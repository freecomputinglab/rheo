//! `sys.inputs.rheo-ligaments`: the harvested ligaments behind an opaque
//! native value, readable only through [`LigamentStore::get`].

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use ecow::EcoString;
use typst::World;
use typst::foundations::{Array, Bytes, Repr, Str, Value};
use typst::syntax::{FileId, RootedPath, VirtualPath, VirtualRoot};
use typst::utils::hash128;
use typst_library::engine::Engine;
use typst_library::foundations::{func, scope, ty};

use crate::build::Ligaments;
use crate::config::RESERVED_LIGAMENTS_INPUT_KEY;

/// Virtual-file prefix whose "contents" are a key's value hash; the world
/// answers it from the store, never from disk.
pub const TOKEN_PREFIX: &str = "__rheo-ligament/";

/// The previous compile's ligaments, as a Typst value.
///
/// Its `Hash` is constant on purpose: `Library` is hashed as one unit by
/// comemo, so any content-dependent hash here would invalidate the whole
/// compile whenever one key changed. Dependence is recorded per key instead,
/// by `get` reading a token file.
#[ty(scope, name = "ligaments")]
#[derive(Debug, Clone)]
pub struct LigamentStore {
    /// Per key: a hash of its value, and the `(page, value)` array itself.
    entries: Arc<HashMap<EcoString, (u128, Value)>>,
    active: bool,
}

impl LigamentStore {
    /// `None` yields an inactive store (a full build): `get` answers `none`.
    pub fn new(ligaments: Option<&Ligaments>) -> Self {
        let entries = ligaments
            .map(|l| {
                l.by_key()
                    .into_iter()
                    .map(|(key, pairs)| {
                        let value = Value::Array(pairs.into_iter().collect::<Array>());
                        (EcoString::from(key), (hash128(&value), value))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            entries: Arc::new(entries),
            active: ligaments.is_some(),
        }
    }

    /// What the world serves for `key`'s token file; changes exactly when
    /// `get(key)` would answer differently.
    pub fn token(&self, key: &str) -> Bytes {
        let bytes = match (self.active, self.entries.get(key)) {
            (false, _) => b"inactive".to_vec(),
            (true, None) => b"absent".to_vec(),
            (true, Some((hash, _))) => hash.to_le_bytes().to_vec(),
        };
        Bytes::new(bytes)
    }
}

impl LigamentStore {
    /// The token file `id` names, or `None` when `id` is no token path.
    pub fn serve(&self, id: FileId) -> Option<Bytes> {
        let hex = id
            .vpath()
            .get_without_slash()
            .strip_prefix(TOKEN_PREFIX)
            .filter(|_| matches!(id.root(), VirtualRoot::Project))?;
        let raw: Vec<u8> = (0..hex.len() / 2)
            .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
            .collect();
        Some(self.token(&String::from_utf8_lossy(&raw)))
    }
}

/// The `FileId` `get` reads to record a dependency on `key`. Hex keeps any
/// key (`:`, `/`, spaces) a single valid path segment.
fn token_id(key: &str) -> FileId {
    let hex: String = key.bytes().map(|b| format!("{b:02x}")).collect();
    let path = VirtualPath::new(format!("{TOKEN_PREFIX}{hex}")).expect("hex path is valid");
    RootedPath::new(VirtualRoot::Project, path).intern()
}

impl LigamentStore {
    /// The store on `sys.inputs.rheo-ligaments` of the library `engine` runs in.
    fn of_library(engine: &Engine) -> Option<Self> {
        let Value::Module(sys) = engine.library.global.scope().get("sys")?.read() else {
            return None;
        };
        let Value::Dict(inputs) = sys.scope().get("inputs")?.read() else {
            return None;
        };
        let Value::Dyn(store) = inputs.get(RESERVED_LIGAMENTS_INPUT_KEY).ok()? else {
            return None;
        };
        store.downcast::<Self>().cloned()
    }
}

#[scope]
impl LigamentStore {
    /// The `(page, value)` pairs attached to `key`, or `none` when this
    /// compile carries no ligaments.
    #[func]
    pub fn get(&self, engine: &mut Engine, key: Str) -> Value {
        // The tracked read is the dependency; its result is irrelevant here.
        let _ = engine.world.file(token_id(&key));
        // Answer from the store of the library compiling NOW, not `self`:
        // `self` may be captured by a closure memoised in an earlier world,
        // whose library hashed identically but held other entries.
        let this = Self::of_library(engine).unwrap_or_else(|| self.clone());
        if !this.active {
            return Value::None;
        }
        this.entries
            .get(key.as_str())
            .map_or_else(|| Value::Array(Array::new()), |(_, v)| v.clone())
    }
}

impl Hash for LigamentStore {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

impl PartialEq for LigamentStore {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.entries, &other.entries) && self.active == other.active
    }
}

impl Repr for LigamentStore {
    fn repr(&self) -> EcoString {
        "ligaments(..)".into()
    }
}

//! Opaque keyed stores on `sys.inputs` with a constant `Hash`, readable only
//! through [`KeyedStore::get`]: `sys.inputs.rheo-ligaments` (the harvested
//! ligaments) and `sys.inputs.rheo-spine` (`spine`/`spine-flat`).

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
use crate::config::{RESERVED_LIGAMENTS_INPUT_KEY, RESERVED_SPINE_INPUT_KEY};

/// What distinguishes one keyed store from another: the reserved `sys.inputs`
/// key it lives under, and the virtual-file prefix whose "contents" are a
/// key's value hash. The world answers that prefix from the store, never from
/// disk.
#[derive(Debug, PartialEq, Eq)]
pub struct StoreKind {
    /// The reserved `sys.inputs` key the store is inserted under.
    pub input_key: &'static str,
    /// The token-path prefix the store's `serve` answers.
    pub token_prefix: &'static str,
    /// What `repr` shows.
    repr: &'static str,
}

/// `sys.inputs.rheo-ligaments`: the previous compile's ligaments.
pub static LIGAMENTS: StoreKind = StoreKind {
    input_key: RESERVED_LIGAMENTS_INPUT_KEY,
    token_prefix: "__rheo-ligament/",
    repr: "ligaments(..)",
};

/// `sys.inputs.rheo-spine`: `spine` and `spine-flat`.
pub static SPINE: StoreKind = StoreKind {
    input_key: RESERVED_SPINE_INPUT_KEY,
    token_prefix: "__rheo-spine/",
    repr: "spine(..)",
};

/// A keyed map of Typst values on `sys.inputs`.
///
/// Its `Hash` is constant on purpose: `Library` is hashed as one unit by
/// comemo, so any content-dependent hash here would invalidate the whole
/// compile whenever one key changed. Dependence is recorded per key instead,
/// by `get` reading a token file.
#[ty(scope, name = "store")]
#[derive(Debug, Clone)]
pub struct KeyedStore {
    kind: &'static StoreKind,
    /// Per key: a hash of its value, and the value itself.
    entries: Arc<HashMap<EcoString, (u128, Value)>>,
    active: bool,
}

impl KeyedStore {
    /// `None` yields an inactive store: `get` answers `none` for every key.
    /// Active, a key absent from `entries` answers an empty array.
    pub fn new(kind: &'static StoreKind, entries: Option<Vec<(EcoString, Value)>>) -> Self {
        let active = entries.is_some();
        let entries = entries
            .unwrap_or_default()
            .into_iter()
            .map(|(key, value)| (key, (hash128(&value), value)))
            .collect();
        Self {
            kind,
            entries: Arc::new(entries),
            active,
        }
    }

    /// The ligament store; `None` (a full build) yields an inactive one.
    pub fn ligaments(ligaments: Option<&Ligaments>) -> Self {
        let entries = ligaments.map(|l| {
            l.by_key()
                .into_iter()
                .map(|(key, pairs)| {
                    let value = Value::Array(pairs.into_iter().collect::<Array>());
                    (EcoString::from(key), value)
                })
                .collect()
        });
        Self::new(&LIGAMENTS, entries)
    }

    /// The spine store; `None` (no `rheo-context` at all) yields an inactive
    /// one.
    pub fn spine(entries: Option<Vec<(EcoString, Value)>>) -> Self {
        Self::new(&SPINE, entries)
    }

    /// Which store this is.
    pub fn kind(&self) -> &'static StoreKind {
        self.kind
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

    /// The token file `id` names, or `None` when `id` is no token path of
    /// this store's kind.
    pub fn serve(&self, id: FileId) -> Option<Bytes> {
        let hex = id
            .vpath()
            .get_without_slash()
            .strip_prefix(self.kind.token_prefix)
            .filter(|_| matches!(id.root(), VirtualRoot::Project))?;
        let raw: Vec<u8> = (0..hex.len() / 2)
            .filter_map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
            .collect();
        Some(self.token(&String::from_utf8_lossy(&raw)))
    }

    /// The `FileId` `get` reads to record a dependency on `key`. Hex keeps
    /// any key (`:`, `/`, spaces) a single valid path segment.
    fn token_id(&self, key: &str) -> FileId {
        let hex: String = key.bytes().map(|b| format!("{b:02x}")).collect();
        let path = VirtualPath::new(format!("{}{hex}", self.kind.token_prefix))
            .expect("hex path is valid");
        RootedPath::new(VirtualRoot::Project, path).intern()
    }

    /// The store of this kind on `sys.inputs` of the library `engine` runs in.
    fn of_library(&self, engine: &Engine) -> Option<Self> {
        let Value::Module(sys) = engine.library.global.scope().get("sys")?.read() else {
            return None;
        };
        let Value::Dict(inputs) = sys.scope().get("inputs")?.read() else {
            return None;
        };
        let Value::Dyn(store) = inputs.get(self.kind.input_key).ok()? else {
            return None;
        };
        store
            .downcast::<Self>()
            .filter(|s| s.kind == self.kind)
            .cloned()
    }
}

#[scope]
impl KeyedStore {
    /// The value under `key`: an empty array when this compile's store has no
    /// such key, `none` when the store is inactive.
    #[func]
    pub fn get(&self, engine: &mut Engine, key: Str) -> Value {
        // The tracked read is the dependency; its result is irrelevant here.
        let _ = engine.world.file(self.token_id(&key));
        // Answer from the store of the library compiling NOW, not `self`:
        // `self` may be captured by a closure memoised in an earlier world,
        // whose library hashed identically but held other entries.
        let this = self.of_library(engine).unwrap_or_else(|| self.clone());
        if !this.active {
            return Value::None;
        }
        this.entries
            .get(key.as_str())
            .map_or_else(|| Value::Array(Array::new()), |(_, v)| v.clone())
    }
}

impl Hash for KeyedStore {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

impl PartialEq for KeyedStore {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && Arc::ptr_eq(&self.entries, &other.entries)
            && self.active == other.active
    }
}

impl Repr for KeyedStore {
    fn repr(&self) -> EcoString {
        self.kind.repr.into()
    }
}

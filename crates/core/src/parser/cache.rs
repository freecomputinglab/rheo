//! Content-addressed caches for the per-file work a build repeats on every
//! rebuild: the package-import scan ([`crate::packages`]), the spine's label
//! extraction ([`crate::reticulate::spine`]), and the parse behind every
//! source the Typst world serves ([`crate::world`]).
//!
//! All three are pure functions of the bytes they are given, so a hit on a
//! 128-bit content hash is indistinguishable from re-running one — changed
//! bytes hash differently and miss. **Nothing here can serve a stale result,
//! only an identical one**, which is why the key is the content rather than a
//! path and mtime.
//!
//! What this buys: a `rheo watch` rebuild redoes all of it over every project
//! file *and* every `.typ` of every imported package, whether or not anything
//! changed, for an answer that differs only for the file the author just
//! saved. On waterline's rookery that was ~650 files scanned and ~150 sources
//! parsed per rebuild.

use super::{ExtractedNodes, ImportInfo, extract_nodes};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use typst::syntax::{FileId, Source};
use typst::utils::hash128;

/// A map from content hash to cached value, bounded by its own capacity.
struct ContentCache<V> {
    entries: Mutex<HashMap<u128, (V, u64)>>,
    clock: AtomicU64,
    /// Entries kept before the cache sheds its coldest half. Sized per cache
    /// by how large one entry is: a file's import list is a handful of
    /// strings, a parsed syntax tree is not.
    capacity: usize,
}

impl<V: Clone> ContentCache<V> {
    fn new(capacity: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            clock: AtomicU64::new(0),
            capacity,
        }
    }

    /// The value for `key`, from the cache or from `compute`.
    ///
    /// `compute` runs outside the lock: it is pure, so two threads racing on
    /// the same miss duplicate the work and agree on the answer, which is
    /// cheaper than holding every other file's lookup behind one parse.
    fn get_or_compute(&self, key: u128, compute: impl FnOnce() -> V) -> V {
        let tick = self.clock.fetch_add(1, Ordering::Relaxed);

        if let Some((value, last_used)) = self.entries.lock().get_mut(&key) {
            *last_used = tick;
            return value.clone();
        }

        let value = compute();
        self.insert(key, value.clone(), tick);
        value
    }

    /// The value stored under `key`, marking it as just read.
    fn get(&self, key: u128) -> Option<V> {
        let tick = self.clock.fetch_add(1, Ordering::Relaxed);
        let mut entries = self.entries.lock();
        let (value, last_used) = entries.get_mut(&key)?;
        *last_used = tick;
        Some(value.clone())
    }

    /// Store `value` under `key`, shedding the coldest half first if full.
    fn put(&self, key: u128, value: V) {
        let tick = self.clock.fetch_add(1, Ordering::Relaxed);
        self.insert(key, value, tick);
    }

    fn insert(&self, key: u128, value: V, tick: u64) {
        let mut entries = self.entries.lock();
        if entries.len() >= self.capacity && !entries.contains_key(&key) {
            shed_coldest_half(&mut entries);
        }
        entries.insert(key, (value, tick));
    }

    /// [`Self::get_or_compute`] keyed by the hash of `text` alone.
    fn get_or_harvest(&self, text: &str, harvest: impl FnOnce() -> V) -> V {
        self.get_or_compute(hash128(text), harvest)
    }
}

/// Drop the half of `entries` least recently read, so a watch session that
/// edits one file thousands of times does not grow without bound.
fn shed_coldest_half<V>(entries: &mut HashMap<u128, (V, u64)>) {
    let mut ticks: Vec<u64> = entries.values().map(|(_, tick)| *tick).collect();
    ticks.sort_unstable();
    let cutoff = ticks[ticks.len() / 2];
    entries.retain(|_, (_, tick)| *tick >= cutoff);
}

/// Process-global, like the `comemo` memo cache and the font book already
/// are: a content hash is a global identity, so nothing is gained by scoping
/// these to a `Build` and a `watch` session's whole point is that they
/// outlive one rebuild.
static PACKAGE_PATHS: LazyLock<ContentCache<Vec<String>>> =
    LazyLock::new(|| ContentCache::new(4096));
static EXTRACTED: LazyLock<ContentCache<ExtractedNodes>> =
    LazyLock::new(|| ContentCache::new(4096));
/// Sized well above one project's working set (its vertebrae plus the package
/// files a compile reaches): a file shed from [`LAST_SOURCES`] is next parsed
/// from scratch, which renumbers its spans and misses every memo downstream.
static SOURCES: LazyLock<ContentCache<Source>> = LazyLock::new(|| ContentCache::new(8192));
/// The last [`Source`] served per file id, the starting point for reparsing
/// an edit incrementally. Same bound as [`SOURCES`] and shed the same way, so
/// a long watch session holds at most one tree per recently used file.
static LAST_SOURCES: LazyLock<ContentCache<Source>> = LazyLock::new(|| ContentCache::new(8192));

/// The `@`-prefixed package import paths `text` names, in encounter order.
///
/// The cached form of parsing `text` and walking it for imports, which is
/// what the `packages` phase does to every project and package `.typ` on
/// every build.
pub fn package_paths(text: &str) -> Vec<String> {
    PACKAGE_PATHS.get_or_harvest(text, || ImportInfo::package_paths(&Source::detached(text)))
}

/// Everything the spine harvests from a vertebra's source — the cached form of
/// [`extract_nodes`] over a freshly parsed `text`.
pub fn extracted(text: &str) -> ExtractedNodes {
    EXTRACTED.get_or_harvest(text, || extract_nodes(&Source::detached(text)))
}

/// The parsed [`Source`] for `id` carrying `text` — the cached form of
/// `Source::new`, which parses eagerly.
///
/// A `Source` is a pure function of its file id and its text, so the key is
/// both. This is what a `watch` rebuild would otherwise redo for every
/// vertebra in the bundle: the Typst world is built fresh per compile and its
/// slots therefore start empty, even though the moulded text of a vertebra
/// nobody touched is byte-for-byte what it was.
///
/// On a miss the last source served for `id` is cloned and edited with
/// `Source::replace`, which reparses only the changed region and so keeps the
/// spans of every unchanged node. That is on purpose: Typst's content hashes
/// include spans, so a from-scratch parse would re-hash everything an edit did
/// not touch. The result's text is still exactly `text`, and the clone is
/// copy-on-write, so a source handed out earlier is never mutated.
pub fn source(id: FileId, text: String) -> Source {
    let source = SOURCES.get_or_compute(hash128(&(id, &text)), || {
        match LAST_SOURCES.get(hash128(&id)) {
            Some(mut previous) => {
                previous.replace(&text);
                previous
            }
            None => Source::new(id, text),
        }
    });
    LAST_SOURCES.put(hash128(&id), source.clone());
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_harvests_once() {
        let cache: ContentCache<usize> = ContentCache::new(64);
        let mut calls = 0;
        for _ in 0..3 {
            let seen = cache.get_or_harvest("#import \"@preview/x:1.0.0\": y", || {
                calls += 1;
                calls
            });
            assert_eq!(seen, 1);
        }
        assert_eq!(calls, 1);
    }

    #[test]
    fn changed_text_misses() {
        let cache: ContentCache<usize> = ContentCache::new(64);
        assert_eq!(cache.get_or_harvest("a", || 1), 1);
        assert_eq!(cache.get_or_harvest("b", || 2), 2);
        assert_eq!(cache.get_or_harvest("a", || 99), 1);
    }

    #[test]
    fn stays_bounded_and_keeps_the_hot_entries() {
        let cache: ContentCache<usize> = ContentCache::new(64);
        // One entry read on every iteration, against a flood of single-use
        // ones: the hot entry must survive the shed.
        for i in 0..256 {
            cache.get_or_harvest("hot", || 0);
            cache.get_or_harvest(&format!("cold {i}"), || i);
        }
        assert!(cache.entries.lock().len() <= 64);
        assert!(cache.entries.lock().contains_key(&hash128("hot")));
    }

    #[test]
    fn package_paths_are_the_uncached_walk() {
        let text = "#import \"@preview/tablex:0.0.6\": tablex\n#import \"@rookery/core:0.1.0\": *";
        let direct = ImportInfo::package_paths(&Source::detached(text));
        assert_eq!(package_paths(text), direct);
        // Second call comes from the cache and must not differ.
        assert_eq!(package_paths(text), direct);
    }

    #[test]
    fn a_source_is_reused_for_identical_text_and_not_across_ids() {
        use typst::syntax::{RootedPath, VirtualPath, VirtualRoot};
        let id = |name: &str| {
            RootedPath::new(VirtualRoot::Project, VirtualPath::new(name).unwrap()).intern()
        };
        let a = id("one.typ");
        let b = id("two.typ");

        let first = source(a, "= Hello\n".to_string());
        let again = source(a, "= Hello\n".to_string());
        assert_eq!(first.text(), again.text());
        assert_eq!(first.id(), a);

        // A different id with the same text is a different source, because a
        // `Source` carries its id and diagnostics are attributed by it.
        assert_eq!(source(b, "= Hello\n".to_string()).id(), b);
        // Changed text for the same id misses rather than serving the old.
        assert_eq!(source(a, "= Goodbye\n".to_string()).text(), "= Goodbye\n");
        assert_eq!(source(a, "= Hello\n".to_string()).text(), "= Hello\n");
    }

    #[test]
    fn an_edit_keeps_the_spans_of_what_it_did_not_touch() {
        use typst::syntax::{RootedPath, VirtualPath, VirtualRoot};
        let id =
            RootedPath::new(VirtualRoot::Project, VirtualPath::new("edit.typ").unwrap()).intern();
        let old = "= Title\n\nFirst paragraph.";
        let new = format!("{old}\n\nmore");

        let first = source(id, old.to_string());
        let first_span = first.root().children().next().unwrap().span();
        let edited = source(id, new.clone());
        assert_eq!(edited.text(), new);
        assert_eq!(edited.root().children().next().unwrap().span(), first_span);

        // The earlier source was not mutated by the edit.
        assert_eq!(first.text(), old);
        // The same new text again is a content-cache hit.
        let again = source(id, new.clone());
        assert_eq!(again.text(), new);
        assert!(std::ptr::eq(again.text().as_ptr(), edited.text().as_ptr()));
    }

    #[test]
    fn extracted_matches_the_uncached_harvest() {
        let text = "= Heading <a>\n\nSee @a and @b.\n";
        let direct = extract_nodes(&Source::detached(text));
        assert_eq!(extracted(text).labels, direct.labels);
        assert_eq!(extracted(text).labels, direct.labels);
    }
}

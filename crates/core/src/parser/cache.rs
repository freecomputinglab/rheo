//! Content-addressed caches for the two per-file harvests on the build's hot
//! path: the package-import scan ([`crate::packages`]) and the spine's label
//! extraction ([`crate::reticulate::spine`]).
//!
//! Both harvests are pure functions of a file's bytes, so a hit on a 128-bit
//! content hash is indistinguishable from re-running one — a changed file
//! hashes differently and misses. **Nothing here can serve a stale result,
//! only an identical one**, which is why the key is the content rather than a
//! path and mtime.
//!
//! What this buys: a `rheo watch` rebuild re-runs both harvests over every
//! project file *and* every `.typ` of every imported package, whether or not
//! anything changed. On waterline's rookery that is ~650 files parsed twice
//! over — the `packages` phase alone was 370ms of a 1.8s rebuild — for an
//! answer that differs only for the one file the author just saved.

use super::{ExtractedNodes, ImportInfo, extract_nodes};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use typst::syntax::Source;
use typst::utils::hash128;

/// Entries kept before the cache sheds its coldest half. The working set is a
/// project's file count plus its packages' — ~650 for waterline's rookery —
/// and only an edited file's *new* content adds an entry, so this bounds a
/// long watch session rather than the build in front of it.
const CAPACITY: usize = 4096;

/// A map from content hash to harvested value, bounded by [`CAPACITY`].
struct ContentCache<V> {
    entries: Mutex<HashMap<u128, (V, u64)>>,
    clock: AtomicU64,
}

impl<V: Clone> ContentCache<V> {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            clock: AtomicU64::new(0),
        }
    }

    /// The harvest of `text`, from the cache or from `harvest`.
    ///
    /// `harvest` runs outside the lock: it is pure, so two threads racing on
    /// the same miss duplicate the work and agree on the answer, which is
    /// cheaper than holding every other file's lookup behind one parse.
    fn get_or_harvest(&self, text: &str, harvest: impl FnOnce() -> V) -> V {
        let key = hash128(text);
        let tick = self.clock.fetch_add(1, Ordering::Relaxed);

        if let Some((value, last_used)) = self.entries.lock().get_mut(&key) {
            *last_used = tick;
            return value.clone();
        }

        let value = harvest();
        let mut entries = self.entries.lock();
        if entries.len() >= CAPACITY {
            shed_coldest_half(&mut entries);
        }
        entries.insert(key, (value.clone(), tick));
        value
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
static PACKAGE_PATHS: LazyLock<ContentCache<Vec<String>>> = LazyLock::new(ContentCache::new);
static EXTRACTED: LazyLock<ContentCache<ExtractedNodes>> = LazyLock::new(ContentCache::new);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_text_harvests_once() {
        let cache: ContentCache<usize> = ContentCache::new();
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
        let cache: ContentCache<usize> = ContentCache::new();
        assert_eq!(cache.get_or_harvest("a", || 1), 1);
        assert_eq!(cache.get_or_harvest("b", || 2), 2);
        assert_eq!(cache.get_or_harvest("a", || 99), 1);
    }

    #[test]
    fn stays_bounded_and_keeps_the_hot_entries() {
        let cache: ContentCache<usize> = ContentCache::new();
        // One entry read on every iteration, against a flood of single-use
        // ones: the hot entry must survive the shed.
        for i in 0..CAPACITY * 2 {
            cache.get_or_harvest("hot", || 0);
            cache.get_or_harvest(&format!("cold {i}"), || i);
        }
        assert!(cache.entries.lock().len() <= CAPACITY);
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
    fn extracted_matches_the_uncached_harvest() {
        let text = "= Heading <a>\n\nSee @a and @b.\n";
        let direct = extract_nodes(&Source::detached(text));
        assert_eq!(extracted(text).labels, direct.labels);
        assert_eq!(extracted(text).labels, direct.labels);
    }
}

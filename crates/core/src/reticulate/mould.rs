//! The Mould stage: rewrite vertebra sources before Typst compilation.
//!
//! A [`SyntaxRewrite`] is a decided edit at a site — a map from the span's
//! current text to its replacement. Producers turn a vertebra's `SyntaxSite`s
//! into `SyntaxRewrite`s. [`Rewrites`] collects them and applies them
//! **end-first** (so earlier byte offsets stay valid). [`VirtualSpine::mould`]
//! runs the producers per vertebra and returns a [`SpineMould`]: the synthesized
//! main plus each vertebra's rewritten body, ready to hand to the Cast stage
//! (Typst bundle compilation).

use super::label_rewrite::LabelRewrite;
use super::spine::{Vertebra, VirtualSpine};
use std::collections::HashMap;
use std::ops::Range;

/// A decided edit at a syntax site: replace the bytes at [`range`](Self::range)
/// with the result of mapping their current text.
///
/// A rewrite kind implements this to describe how one class of site is edited;
/// moulding is agnostic to which kinds exist.
pub trait SyntaxRewrite {
    /// Byte range of the span to replace in the original source.
    fn range(&self) -> Range<usize>;
    /// Map the span's current text to its replacement ("what it was" → "what it
    /// should be").
    fn map(&self, original: &str) -> String;
}

/// A set of rewrites over one source, applied lazily and end-first.
pub struct Rewrites(pub Vec<Box<dyn SyntaxRewrite>>);

impl Rewrites {
    /// `true` when there are no rewrites (the source is left untouched).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Apply every rewrite to `source`, splicing from the end so earlier byte
    /// offsets stay valid. Each replacement is computed from the **original**
    /// span text. Rewrite ranges must not overlap.
    pub fn apply(mut self, source: &str) -> String {
        // End-first: largest start offset first.
        self.0.sort_by(|a, b| b.range().start.cmp(&a.range().start));
        debug_assert!(
            self.0
                .windows(2)
                .all(|w| w[1].range().end <= w[0].range().start),
            "SyntaxRewrite ranges must not overlap"
        );
        let mut out = source.to_string();
        for rewrite in &self.0 {
            let range = rewrite.range();
            let replacement = rewrite.map(&source[range.clone()]);
            out.replace_range(range, &replacement);
        }
        out
    }
}

/// The moulded virtual Typst file handed to the Cast stage.
pub struct SpineMould {
    /// The synthesized `#document(…)[…#include…]` main (see [`VirtualSpine::bundle_source`]).
    pub main: String,
    /// Rewritten source per vertebra, keyed by its `#include` path (`Vertebra::rel_path`).
    /// A vertebra with no rewrites is omitted, so the world reads it from disk unchanged.
    pub bodies: HashMap<String, String>,
}

impl Vertebra {
    /// Collect this vertebra's Mould rewrites and apply them to its source.
    ///
    /// Returns `None` when there are no rewrites (an identity mould), so the
    /// caller can omit the vertebra from the overlay and let it disk-read.
    pub fn mould(&self, prefix_labels: bool) -> Option<String> {
        let rewrites = self.rewrites(prefix_labels);
        if rewrites.is_empty() {
            return None;
        }
        Some(rewrites.apply(&self.source))
    }

    /// Gather the rewrites every Mould producer wants applied to this vertebra.
    ///
    /// The producer set is the extension seam: rewrite producers append here
    /// (gated by `prefix_labels`), and format plugins may contribute their own.
    fn rewrites(&self, prefix_labels: bool) -> Rewrites {
        let mut list: Vec<Box<dyn SyntaxRewrite>> = Vec::new();
        if prefix_labels {
            list.extend(
                LabelRewrite::collect(self)
                    .into_iter()
                    .map(|r| Box::new(r) as Box<dyn SyntaxRewrite>),
            );
        }
        Rewrites(list)
    }
}

impl VirtualSpine {
    /// Mould the spine: synthesize the main and rewrite each vertebra's body.
    ///
    /// When `prefix_labels` is disabled (or a vertebra has no rewrites), the
    /// vertebra is omitted from `bodies` and served from disk unchanged.
    pub fn mould(&self, prefix_labels: bool) -> SpineMould {
        let main = self.bundle_source().to_string();
        let mut bodies = HashMap::new();
        for vertebra in &self.vertebrae {
            if let Some(body) = vertebra.mould(prefix_labels) {
                bodies.insert(vertebra.rel_path.clone(), body);
            }
        }
        SpineMould { main, bodies }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rewrite that replaces its span with a fixed string.
    struct Replace {
        range: Range<usize>,
        text: &'static str,
    }
    impl SyntaxRewrite for Replace {
        fn range(&self) -> Range<usize> {
            self.range.clone()
        }
        fn map(&self, _original: &str) -> String {
            self.text.to_string()
        }
    }

    /// A rewrite that upper-cases its span — proves `map` sees the original text.
    struct Upper {
        range: Range<usize>,
    }
    impl SyntaxRewrite for Upper {
        fn range(&self) -> Range<usize> {
            self.range.clone()
        }
        fn map(&self, original: &str) -> String {
            original.to_uppercase()
        }
    }

    #[test]
    fn apply_splices_all_end_first() {
        // Rewrites supplied out of order; apply must handle them end-first.
        let rewrites = Rewrites(vec![
            Box::new(Replace {
                range: 0..3,
                text: "[1]",
            }),
            Box::new(Replace {
                range: 6..9,
                text: "[3]",
            }),
            Box::new(Replace {
                range: 3..6,
                text: "[2]",
            }),
        ]);
        assert_eq!(rewrites.apply("abcABCxyz"), "[1][2][3]");
    }

    #[test]
    fn map_receives_the_original_span() {
        let rewrites = Rewrites(vec![Box::new(Upper { range: 0..5 })]);
        assert_eq!(rewrites.apply("hello world"), "HELLO world");
    }

    #[test]
    fn empty_rewrites_leave_source_untouched() {
        let rewrites = Rewrites(vec![]);
        assert!(rewrites.is_empty());
        assert_eq!(rewrites.apply("unchanged"), "unchanged");
    }

    #[test]
    #[should_panic(expected = "must not overlap")]
    fn overlapping_rewrites_panic_in_debug() {
        let rewrites = Rewrites(vec![
            Box::new(Replace {
                range: 0..4,
                text: "x",
            }),
            Box::new(Replace {
                range: 2..6,
                text: "y",
            }),
        ]);
        let _ = rewrites.apply("abcdefgh");
    }
}

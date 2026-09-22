//! Where every byte of a synthesized Typst source came from.
//!
//! `SourceInjector` prepends and appends Typst rheo generates around an
//! authored file's own text. A [`SourceMap`] records which stretches of the
//! result are rheo's own and which are carried verbatim from an authored
//! file, so a diagnostic's byte range can be translated back to the file the
//! author can actually open.
//!
//! A future Mould rewrite producer that changes a vertebra's overlay text
//! ahead of injection will need to contribute its own segments here; today
//! Mould is an identity transform and this type sees only injection.

use std::ops::Range;
use std::sync::Arc;

/// An authored file a synthesized source carries verbatim.
#[derive(Debug, Clone)]
pub struct AuthoredFile {
    /// Project-relative display name — what a diagnostic should print.
    pub name: String,
    /// The authored text in full, so a renderer can count lines from its start.
    pub text: Arc<str>,
}

/// One contiguous run of a synthesized source: either bytes rheo generated,
/// or bytes carried verbatim from an authored file starting at `start`
/// within it.
#[derive(Debug, Clone)]
enum Origin {
    Injected,
    Authored { file: AuthoredFile, start: usize },
}

#[derive(Debug, Clone)]
struct Segment {
    range: Range<usize>,
    origin: Origin,
}

/// Where every byte of one synthesized Typst file came from.
#[derive(Debug, Default, Clone)]
pub struct SourceMap {
    segments: Vec<Segment>,
    len: usize,
}

impl SourceMap {
    /// Append `len` bytes of Typst rheo generated itself.
    pub fn push_injected(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let range = self.len..self.len + len;
        self.len += len;
        self.segments.push(Segment {
            range,
            origin: Origin::Injected,
        });
    }

    /// Append `len` bytes carried verbatim from `file`, starting at byte
    /// `start` within it.
    pub fn push_authored(&mut self, file: AuthoredFile, start: usize, len: usize) {
        if len == 0 {
            return;
        }
        let range = self.len..self.len + len;
        self.len += len;
        self.segments.push(Segment {
            range,
            origin: Origin::Authored { file, start },
        });
    }

    /// Append every segment of `other`, continuing immediately after this
    /// map's own bytes — for splicing one synthesized source's map onto the
    /// tail of another (the bundle main's own map onto the scaffolding
    /// injected around it).
    pub fn append_shifted(&mut self, other: &SourceMap) {
        for segment in &other.segments {
            let len = segment.range.end - segment.range.start;
            match &segment.origin {
                Origin::Injected => self.push_injected(len),
                Origin::Authored { file, start } => self.push_authored(file.clone(), *start, len),
            }
        }
    }

    /// Translate a byte range in the synthesized text back to the authored
    /// file it came from. `None` when the range begins in injected text or
    /// straddles a segment boundary — a span rheo cannot honestly attribute
    /// must not be guessed at.
    pub fn resolve(&self, range: &Range<usize>) -> Option<(&AuthoredFile, Range<usize>)> {
        let segment = self
            .segments
            .iter()
            .find(|s| s.range.start <= range.start && range.start < s.range.end)?;
        if range.end > segment.range.end {
            return None;
        }
        match &segment.origin {
            Origin::Injected => None,
            Origin::Authored { file, start } => {
                let offset = range.start - segment.range.start;
                let authored_start = start + offset;
                let authored_end = authored_start + (range.end - range.start);
                Some((file, authored_start..authored_end))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, text: &str) -> AuthoredFile {
        AuthoredFile {
            name: name.to_string(),
            text: text.into(),
        }
    }

    #[test]
    fn resolves_a_range_inside_an_authored_segment() {
        let mut map = SourceMap::default();
        map.push_injected(10);
        map.push_authored(file("a.typ", "hello world"), 0, 11);

        let (resolved, range) = map.resolve(&(12..17)).expect("resolves");
        assert_eq!(resolved.name, "a.typ");
        assert_eq!(range, 2..7);
    }

    #[test]
    fn returns_none_for_a_range_in_an_injected_segment() {
        let mut map = SourceMap::default();
        map.push_injected(10);
        map.push_authored(file("a.typ", "hello"), 0, 5);

        assert!(map.resolve(&(0..5)).is_none());
    }

    #[test]
    fn returns_none_for_a_range_straddling_a_boundary() {
        let mut map = SourceMap::default();
        map.push_authored(file("a.typ", "hello"), 0, 5);
        map.push_injected(5);

        assert!(map.resolve(&(3..7)).is_none());
    }

    #[test]
    fn append_shifted_continues_the_other_maps_segments_at_this_maps_own_end() {
        let mut inner = SourceMap::default();
        inner.push_injected(4);
        inner.push_authored(file("a.typ", "hello"), 0, 5);

        let mut outer = SourceMap::default();
        outer.push_injected(3);
        outer.append_shifted(&inner);

        // The authored segment now starts at 3 (outer's own prefix) + 4
        // (inner's own injected prefix) = 7.
        let (resolved, range) = outer.resolve(&(7..12)).expect("resolves");
        assert_eq!(resolved.name, "a.typ");
        assert_eq!(range, 0..5);
        assert!(outer.resolve(&(0..3)).is_none());
    }
}

use std::ops::Range;
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// Find all code block ranges in the source using AST traversal.
///
/// Returns byte ranges of all Raw nodes (code blocks and inline code).
pub fn find_code_block_ranges(source: &Source) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    collect_raw_ranges(source.root(), &mut ranges, 0);
    ranges
}

fn collect_raw_ranges(node: &SyntaxNode, ranges: &mut Vec<Range<usize>>, offset: usize) {
    let node_len = node.len();

    if node.kind() == SyntaxKind::Raw {
        ranges.push(offset..(offset + node_len));
    }

    let mut child_offset = offset;
    for child in node.children() {
        let child_len = child.len();
        collect_raw_ranges(child, ranges, child_offset);
        child_offset += child_len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges_overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
        a.start < b.end && b.start < a.end
    }

    fn overlaps_with_any(range: &Range<usize>, code_ranges: &[Range<usize>]) -> bool {
        code_ranges
            .iter()
            .any(|code_range| ranges_overlap(range, code_range))
    }

    #[test]
    fn test_ranges_overlap() {
        assert!(ranges_overlap(&(0..10), &(5..15)));
        assert!(ranges_overlap(&(5..15), &(0..10)));
        assert!(ranges_overlap(&(0..10), &(0..10)));
        assert!(!ranges_overlap(&(0..10), &(10..20)));
        assert!(!ranges_overlap(&(10..20), &(0..10)));
    }

    #[test]
    fn test_overlaps_with_any() {
        let ranges = vec![10..20, 30..40, 50..60];

        assert!(overlaps_with_any(&(15..25), &ranges));
        assert!(overlaps_with_any(&(35..45), &ranges));
        assert!(overlaps_with_any(&(55..65), &ranges));
        assert!(!overlaps_with_any(&(20..30), &ranges));
        assert!(!overlaps_with_any(&(0..10), &ranges));
    }
}

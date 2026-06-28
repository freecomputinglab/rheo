//! Source code serialization utilities (DEPRECATED link transformation)
//!
//! This module previously contained link transformation logic for the old per-file
//! compilation path. The new bundle compilation path (VirtualSpine + Typst @ref)
//! handles cross-file references natively, making link transformation obsolete.
//!
//! The code block finding utilities are retained for potential future use.

use std::ops::Range;
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// Apply link transformations to source code (DEPRECATED).
///
/// The new bundle compilation path handles cross-file references via Typst @ref,
/// making link transformation unnecessary. This function returns the source unchanged.
#[deprecated(note = "No replacement needed for bundle compilation")]
pub fn apply_transformations(
    source: &str,
    _transformations: &[(Range<usize>, crate::reticulate::types::LinkTransform)],
    _code_block_ranges: &[Range<usize>],
) -> String {
    source.to_string()
}

/// Find all code block ranges in the source using AST traversal
///
/// Returns byte ranges of all Raw nodes (code blocks and inline code).
/// This function is retained for potential future use.
pub fn find_code_block_ranges(source: &Source) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    collect_raw_ranges(source.root(), &mut ranges, 0);
    ranges
}

/// Recursively collect byte ranges of all Raw nodes (code blocks and inline code)
fn collect_raw_ranges(node: &SyntaxNode, ranges: &mut Vec<Range<usize>>, offset: usize) {
    let node_len = node.len();

    // If this is a Raw node, add its byte range
    if node.kind() == SyntaxKind::Raw {
        ranges.push(offset..(offset + node_len));
    }

    // Recurse into children
    let mut child_offset = offset;
    for child in node.children() {
        let child_len = child.len();
        collect_raw_ranges(child, ranges, child_offset);
        child_offset += child_len;
    }
}

/// Check if a range overlaps with any range in a list
fn overlaps_with_any(range: &Range<usize>, code_ranges: &[Range<usize>]) -> bool {
    code_ranges
        .iter()
        .any(|code_range| ranges_overlap(range, code_range))
}

/// Check if two ranges overlap
fn ranges_overlap(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(overlaps_with_any(&(15..25), &ranges)); // Overlaps first
        assert!(overlaps_with_any(&(35..45), &ranges)); // Overlaps second
        assert!(overlaps_with_any(&(55..65), &ranges)); // Overlaps third
        assert!(!overlaps_with_any(&(20..30), &ranges)); // No overlap
        assert!(!overlaps_with_any(&(0..10), &ranges)); // No overlap
    }
}

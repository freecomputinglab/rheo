use super::types::LinkTransform;
use std::ops::Range;
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// Apply link transformations to source code
///
/// Applies transformations by replacing text at specified byte ranges.
/// Transformations that overlap with code blocks are filtered out.
/// Replacements are applied back-to-front to preserve byte offsets.
///
/// # Arguments
/// * `source` - Original source text
/// * `transformations` - List of (byte_range, transform) tuples
/// * `code_block_ranges` - Byte ranges of code blocks to protect
pub fn apply_transformations(
    source: &str,
    transformations: &[(Range<usize>, LinkTransform)],
    code_block_ranges: &[Range<usize>],
) -> String {
    // Filter out transformations that overlap with code blocks
    let mut active_transforms: Vec<_> = transformations
        .iter()
        .filter(|(range, _)| !overlaps_with_any(range, code_block_ranges))
        .collect();

    // Sort by byte range (back-to-front for stable offsets)
    active_transforms.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));

    // Build result string by applying transformations
    let mut result = source.to_string();

    for (range, transform) in active_transforms {
        // Get the original link text
        let original = &source[range.clone()];

        // Compute replacement text based on transformation
        let replacement = match transform {
            LinkTransform::Remove { body } => {
                // Just the body text in brackets: [body]
                format!("[{}]", body)
            }
            LinkTransform::ReplaceUrl { new_url } => {
                // Replace URL but keep the rest of the syntax
                // Original: #link("old.typ")[body]
                // New:      #link("new.typ")[body]
                replace_url_in_link(original, new_url, false)
            }
            LinkTransform::ReplaceUrlWithLabel { new_label } => {
                // Replace URL but keep the rest of the syntax
                // Original: #link("old.typ")[body]
                // New:      #link(<label>)[body]
                replace_url_in_link(original, new_label, true)
            }

            LinkTransform::KeepOriginal => {
                // No change
                original.to_string()
            }
        };

        // Replace in result (safe because we're going back-to-front)
        result.replace_range(range.clone(), &replacement);
    }

    result
}

/// Find all code block ranges in the source using AST traversal
///
/// Returns byte ranges of all Raw nodes (code blocks and inline code).
/// Links within these ranges should be preserved unchanged.
pub fn find_code_block_ranges(source: &Source) -> Vec<Range<usize>> {
    let root = typst::syntax::parse(source.text());
    let mut ranges = Vec::new();
    collect_raw_ranges(&root, &mut ranges, 0);
    ranges
}

/// Recursively collect byte ranges of all Raw nodes (code blocks and inline code)
fn collect_raw_ranges(node: &SyntaxNode, ranges: &mut Vec<Range<usize>>, offset: usize) {
    let node_len = node.text().len();

    // If this is a Raw node, add its byte range
    if node.kind() == SyntaxKind::Raw {
        ranges.push(offset..(offset + node_len));
    }

    // Recurse into children
    let mut child_offset = offset;
    for child in node.children() {
        let child_len = child.text().len();
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

/// Replace the URL in a link while preserving the rest
///
/// Handles both syntaxes:
/// - `#link("url")[body]` → `#link("new_url")[body]`
/// - `#link("url", body)` → `#link("new_url", body)`
fn replace_url_in_link(original: &str, new_url: &str, is_label: bool) -> String {
    // Find the opening quote after #link(
    if let Some(quote_start) = original.find('(') {
        let after_paren = &original[quote_start + 1..];

        // Find the first quote
        if let Some(first_quote) = after_paren.find('"') {
            let before_url =
                &original[..quote_start + 1 + first_quote + (if is_label { 0 } else { 1 })];

            // Find the closing quote
            let after_first_quote = &after_paren[first_quote + 1..];
            if let Some(closing_quote) = after_first_quote.find('"') {
                let after_url =
                    &after_first_quote[closing_quote + (if is_label { 1 } else { 0 })..];

                // Reconstruct: #link("new_url")[body] or #link("new_url", body)
                return format!("{}{}{}", before_url, new_url, after_url);
            }
        }
    }

    // Fallback: return original if parsing failed
    original.to_string()
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

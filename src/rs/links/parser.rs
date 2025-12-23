use typst::syntax::{Source, SyntaxNode, SyntaxKind};
use crate::links::types::LinkInfo;

/// Extract all links from Typst source by parsing and traversing AST
pub fn extract_links(source: &Source) -> Vec<LinkInfo> {
    let root = typst::syntax::parse(source.text());
    let mut links = Vec::new();
    extract_links_from_node(&root, &mut links);
    links
}

fn extract_links_from_node(node: &SyntaxNode, links: &mut Vec<LinkInfo>) {
    // Recursively traverse all nodes
    for child in node.children() {
        // Check if this is a function call
        if child.kind() == SyntaxKind::FuncCall {
            if let Some(link_info) = parse_link_call(&child) {
                links.push(link_info);
            }
        }

        // Recurse into children
        extract_links_from_node(&child, links);
    }
}

fn parse_link_call(node: &SyntaxNode) -> Option<LinkInfo> {
    // Parse #link("url")[body] or #link("url", body)
    // Extract:
    // 1. Function name (must be "link")
    // 2. URL argument (first string argument)
    // 3. Body text (from content block or second argument)
    // 4. Span and byte range

    // Get function name
    let ident = node.children()
        .find(|n| n.kind() == SyntaxKind::Ident)?;
    if ident.text() != "link" {
        return None;
    }

    // Get arguments
    let args = node.children()
        .find(|n| n.kind() == SyntaxKind::Args)?;

    // Extract URL (first string argument)
    let url = extract_first_string_arg(&args)?;

    // Extract body text
    let body = extract_link_body(node)?;

    // Get span and byte range
    let span = node.span();
    // Use span's range if available, otherwise use empty range
    let byte_range = span.range().unwrap_or(0..0);

    Some(LinkInfo {
        url,
        body,
        span,
        byte_range,
    })
}

fn extract_first_string_arg(args: &SyntaxNode) -> Option<String> {
    for child in args.children() {
        if child.kind() == SyntaxKind::Str {
            // Remove quotes
            let text = child.text();
            return Some(text.trim_matches('"').to_string());
        }
    }
    None
}

fn extract_link_body(func_call: &SyntaxNode) -> Option<String> {
    // The ContentBlock is inside the Args node as the second argument
    let args = func_call.children()
        .find(|n| n.kind() == SyntaxKind::Args)?;

    // Find ContentBlock inside Args
    let content_block = args.children()
        .find(|n| n.kind() == SyntaxKind::ContentBlock)?;

    // Extract text from inside the ContentBlock
    // The structure is: ContentBlock -> Markup -> Text
    extract_text_from_node(&content_block)
}

fn extract_text_from_node(node: &SyntaxNode) -> Option<String> {
    // If this is a Text node, return its content
    if node.kind() == SyntaxKind::Text {
        return Some(node.text().to_string());
    }

    // Otherwise, recursively search children for text
    for child in node.children() {
        if let Some(text) = extract_text_from_node(&child) {
            return Some(text);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::Source;

    #[test]
    fn test_extract_link_with_content_block() {
        let source = Source::detached(r#"#link("./file.typ")[text]"#);
        let links = extract_links(&source);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "./file.typ");
        assert_eq!(links[0].body, "text");
    }

    #[test]
    fn test_extract_multiple_links() {
        let source = Source::detached(r#"
            Some text #link("./file1.typ")[first] and more
            #link("./file2.typ")[second] content.
        "#);
        let links = extract_links(&source);

        assert_eq!(links.len(), 2);
        assert_eq!(links[0].url, "./file1.typ");
        assert_eq!(links[0].body, "first");
        assert_eq!(links[1].url, "./file2.typ");
        assert_eq!(links[1].body, "second");
    }

    #[test]
    fn test_no_links() {
        let source = Source::detached("Just plain text with no links");
        let links = extract_links(&source);

        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_external_urls() {
        let source = Source::detached(r#"#link("https://example.com")[external]"#);
        let links = extract_links(&source);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "https://example.com");
        assert_eq!(links[0].body, "external");
    }
}

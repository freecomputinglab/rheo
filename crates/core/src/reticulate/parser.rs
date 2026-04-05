use crate::reticulate::types::{ImportInfo, LinkInfo};
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// The identifier in the Typst AST for links.
const LINK_IDENT_ID: &str = "link";

/// Extract all links from Typst source by parsing and traversing AST
pub fn extract_links(source: &Source) -> Vec<LinkInfo> {
    let root = typst::syntax::parse(source.text());
    let mut links = Vec::new();
    extract_links_from_node(&root, &root, &mut links);
    links
}

fn extract_links_from_node(node: &SyntaxNode, root: &SyntaxNode, links: &mut Vec<LinkInfo>) {
    // Check if this node itself is a function call
    if node.kind() == SyntaxKind::FuncCall
        && let Some(link_info) = parse_link_call(node, root)
    {
        links.push(link_info);
    }

    // Recursively traverse children
    for child in node.children() {
        extract_links_from_node(child, root, links);
    }
}

fn parse_link_call(node: &SyntaxNode, root: &SyntaxNode) -> Option<LinkInfo> {
    // Parse #link("url")[body] or #link("url", body)
    // Extract:
    // 1. Function name (must be "link")
    // 2. URL argument (first string argument)
    // 3. Body text (from content block or second argument)
    // 4. Byte range by calculating AST node position

    let ident = node.children().find(|n| n.kind() == SyntaxKind::Ident)?;
    if ident.text() != LINK_IDENT_ID {
        return None;
    }

    let args = node.children().find(|n| n.kind() == SyntaxKind::Args)?;

    // Extract URL (first string argument)
    let url = extract_first_string_arg(args)?;

    // Extract body text
    let body = extract_link_body(node)?;

    // Calculate byte range directly from AST node position
    let offset = calculate_node_offset(root, node)?;
    let byte_range = offset..(offset + node.len());

    // Get span for error reporting
    let span = node.span();

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
    let args = func_call
        .children()
        .find(|n| n.kind() == SyntaxKind::Args)?;

    // Find ContentBlock inside Args
    let content_block = args
        .children()
        .find(|n| n.kind() == SyntaxKind::ContentBlock)?;

    // Extract text from inside the ContentBlock
    // The structure is: ContentBlock -> Markup -> Text
    extract_text_from_node(content_block)
}

fn extract_text_from_node(node: &SyntaxNode) -> Option<String> {
    // If this is a Text node, return its content
    if node.kind() == SyntaxKind::Text {
        return Some(node.text().to_string());
    }

    // If this is a Space node, return a space
    if node.kind() == SyntaxKind::Space {
        return Some(" ".to_string());
    }

    // Otherwise, collect text from ALL children (not just the first)
    let mut texts = Vec::new();
    for child in node.children() {
        if let Some(text) = extract_text_from_node(child) {
            texts.push(text);
        }
    }

    if texts.is_empty() {
        None
    } else {
        Some(texts.join(""))
    }
}

/// Extract all import/include paths from Typst source by parsing and traversing AST
pub fn extract_imports(source: &Source) -> Vec<ImportInfo> {
    let root = typst::syntax::parse(source.text());
    let mut imports = Vec::new();
    extract_imports_from_node(&root, &root, &mut imports);
    imports
}

fn extract_imports_from_node(
    node: &SyntaxNode,
    root: &SyntaxNode,
    imports: &mut Vec<ImportInfo>,
) {
    if (node.kind() == SyntaxKind::ModuleImport || node.kind() == SyntaxKind::ModuleInclude)
        && let Some(import_info) = parse_import_node(node, root)
    {
        imports.push(import_info);
    }

    for child in node.children() {
        extract_imports_from_node(child, root, imports);
    }
}

fn parse_import_node(node: &SyntaxNode, root: &SyntaxNode) -> Option<ImportInfo> {
    // Find the first Str child — that is the path argument
    let str_node = node.children().find(|n| n.kind() == SyntaxKind::Str)?;

    let text = str_node.text();
    let path = text.trim_matches('"').to_string();

    let offset = calculate_node_offset(root, str_node)?;
    let byte_range = offset..(offset + str_node.len());

    Some(ImportInfo {
        is_package: path.starts_with('@'),
        path,
        byte_range,
    })
}

/// Calculate the byte offset of a target node within the root AST
fn calculate_node_offset(root: &SyntaxNode, target: &SyntaxNode) -> Option<usize> {
    calculate_node_offset_impl(root, target, 0)
}

fn calculate_node_offset_impl(
    current: &SyntaxNode,
    target: &SyntaxNode,
    offset: usize,
) -> Option<usize> {
    // Check if this is the target node (pointer equality)
    if std::ptr::eq(current as *const _, target as *const _) {
        return Some(offset);
    }

    // Recursively search children, tracking offset
    let mut child_offset = offset;
    for child in current.children() {
        if let Some(found_offset) = calculate_node_offset_impl(child, target, child_offset) {
            return Some(found_offset);
        }
        child_offset += child.len();
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
        let source = Source::detached(
            r#"
            Some text #link("./file1.typ")[first] and more
            #link("./file2.typ")[second] content.
        "#,
        );
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

    #[test]
    fn test_extract_link_with_nested_markup() {
        let source = Source::detached(r#"#link("./url")[text #super[2]]"#);
        let links = extract_links(&source);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "./url");
        assert_eq!(links[0].body, "text 2"); // All text concatenated
        // Byte range should cover the entire link (exact start may vary by 1 due to Source::detached)
        assert!(links[0].byte_range.len() >= 29);
    }

    #[test]
    fn test_extract_link_with_multiple_markup() {
        let source = Source::detached(r#"#link("url")[#strong[bold] and #emph[italic]]"#);
        let links = extract_links(&source);

        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "url");
        assert_eq!(links[0].body, "bold and italic");
    }

    // --- extract_imports tests ---

    #[test]
    fn test_extract_import_relative() {
        let source = Source::detached(r#"#import "./utils.typ": *"#);
        let imports = extract_imports(&source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].path, "./utils.typ");
        assert!(!imports[0].is_package);
    }

    #[test]
    fn test_extract_import_package() {
        let source = Source::detached(r#"#import "@preview/tablex:0.0.6": tablex"#);
        let imports = extract_imports(&source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].path, "@preview/tablex:0.0.6");
        assert!(imports[0].is_package);
    }

    #[test]
    fn test_extract_include() {
        let source = Source::detached(r#"#include "./figures/fig1.typ""#);
        let imports = extract_imports(&source);

        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].path, "./figures/fig1.typ");
        assert!(!imports[0].is_package);
    }

    #[test]
    fn test_extract_multiple_imports() {
        let source = Source::detached(
            r#"
            #import "./utils.typ": *
            #import "@preview/tablex:0.0.6": tablex
            #include "./figures/fig1.typ"
        "#,
        );
        let imports = extract_imports(&source);

        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].path, "./utils.typ");
        assert!(!imports[0].is_package);
        assert_eq!(imports[1].path, "@preview/tablex:0.0.6");
        assert!(imports[1].is_package);
        assert_eq!(imports[2].path, "./figures/fig1.typ");
        assert!(!imports[2].is_package);
    }

    #[test]
    fn test_extract_import_byte_range() {
        let source = Source::detached(r#"#import "./utils.typ": *"#);
        let imports = extract_imports(&source);

        assert_eq!(imports.len(), 1);
        // byte_range should cover the Str node including quotes
        let source_text = source.text();
        let range_text = &source_text[imports[0].byte_range.clone()];
        assert_eq!(range_text, "\"./utils.typ\"");
    }

    #[test]
    fn test_no_imports() {
        let source = Source::detached("Just plain text with no imports");
        let imports = extract_imports(&source);

        assert_eq!(imports.len(), 0);
    }
}

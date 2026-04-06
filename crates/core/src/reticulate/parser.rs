use crate::reticulate::types::{ImportInfo, LinkInfo};
use crate::reticulate::validator::is_relative_typ_link;
use std::collections::HashMap;
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// The identifier in the Typst AST for links.
const LINK_IDENT_ID: &str = "link";

/// Maps a wrapper function name to the index of its parameter that is passed
/// as the URL to the inner `link()` call.
pub type WrapperMap = HashMap<String, usize>;

/// Maps a let-binding name to its constant `.typ` string value and the
/// byte range of the `Str` node in the source. Only file-scope bindings
/// of the form `#let x = "./something.typ"` are tracked.
pub type UrlBindingMap = HashMap<String, (String, std::ops::Range<usize>)>;

/// Extract all links from Typst source by parsing and traversing AST.
///
/// Also detects same-file wrapper functions (`#let f(x) = link(x, ...)`) so
/// that calls like `#f("url")` are recognised as links.
pub fn extract_links(source: &Source) -> Vec<LinkInfo> {
    extract_nodes(source).links
}

/// Extract all import/include paths from Typst source by parsing and
/// traversing AST.
pub fn extract_imports(source: &Source) -> Vec<ImportInfo> {
    extract_nodes(source).imports
}

/// Result of a single-pass AST extraction.
pub struct ExtractedNodes {
    pub links: Vec<LinkInfo>,
    pub imports: Vec<ImportInfo>,
}

/// Single-pass extraction of both links and imports from Typst source.
///
/// Parses the source exactly once and traverses the AST once to collect
/// both link info and import info.
pub fn extract_nodes(source: &Source) -> ExtractedNodes {
    let root = typst::syntax::parse(source.text());
    let wrappers = collect_link_wrappers(&root);
    let url_bindings = collect_url_bindings(&root);
    let mut links = Vec::new();
    let mut imports = Vec::new();
    extract_from_node(
        &root,
        &root,
        &mut links,
        &mut imports,
        &wrappers,
        &url_bindings,
    );
    ExtractedNodes { links, imports }
}

/// Combined single-pass traversal that collects both links and imports.
fn extract_from_node(
    node: &SyntaxNode,
    root: &SyntaxNode,
    links: &mut Vec<LinkInfo>,
    imports: &mut Vec<ImportInfo>,
    wrappers: &WrapperMap,
    url_bindings: &UrlBindingMap,
) {
    // Collect link info from function calls
    if node.kind() == SyntaxKind::FuncCall
        && let Some(link_info) = parse_link_call(node, root, wrappers, url_bindings)
    {
        links.push(link_info);
    }

    // Collect import/include info
    if (node.kind() == SyntaxKind::ModuleImport || node.kind() == SyntaxKind::ModuleInclude)
        && let Some(import_info) = parse_import_node(node, root)
    {
        imports.push(import_info);
    }

    // Recursively traverse children
    for child in node.children() {
        extract_from_node(child, root, links, imports, wrappers, url_bindings);
    }
}

fn parse_link_call(
    node: &SyntaxNode,
    root: &SyntaxNode,
    wrappers: &WrapperMap,
    url_bindings: &UrlBindingMap,
) -> Option<LinkInfo> {
    let ident = node.children().find(|n| n.kind() == SyntaxKind::Ident)?;

    let (url_param_index, is_wrapper) = if ident.text() == LINK_IDENT_ID {
        (0, false)
    } else if let Some(&idx) = wrappers.get(ident.text().as_str()) {
        (idx, true)
    } else {
        return None;
    };

    let args = node.children().find(|n| n.kind() == SyntaxKind::Args)?;

    // Try to extract a literal string arg first
    if let Some((url, str_node)) = extract_nth_string_arg_with_node(args, url_param_index) {
        if is_wrapper {
            let offset = calculate_node_offset(root, str_node)?;
            let byte_range = offset..(offset + str_node.len());
            Some(LinkInfo {
                url,
                body: String::new(),
                span: node.span(),
                byte_range,
                is_wrapper_call: true,
            })
        } else {
            let body = extract_link_body(node)?;
            let offset = calculate_node_offset(root, node)?;
            let byte_range = offset..(offset + node.len());
            Some(LinkInfo {
                url,
                body,
                span: node.span(),
                byte_range,
                is_wrapper_call: false,
            })
        }
    } else {
        // No literal string — try to resolve a let-bound variable
        let var_name = extract_nth_ident_arg(args, url_param_index)?;
        let (url, str_byte_range) = url_bindings.get(var_name.as_str())?;
        Some(LinkInfo {
            url: url.clone(),
            body: String::new(),
            span: node.span(),
            byte_range: str_byte_range.clone(),
            is_wrapper_call: true, // reuse flag: rewrite via ReplaceStringLiteralInPlace
        })
    }
}

/// Return the text and node reference of the n-th positional `Str` argument.
fn extract_nth_string_arg_with_node(args: &SyntaxNode, n: usize) -> Option<(String, &SyntaxNode)> {
    let mut pos = 0;
    for child in args.children() {
        // Skip structural tokens and named args
        match child.kind() {
            SyntaxKind::LeftParen
            | SyntaxKind::RightParen
            | SyntaxKind::Comma
            | SyntaxKind::Space
            | SyntaxKind::Named => continue,
            _ => {}
        }
        if pos == n {
            if child.kind() == SyntaxKind::Str {
                let text = child.text().trim_matches('"').to_string();
                return Some((text, child));
            }
            return None; // n-th positional arg is not a string
        }
        pos += 1;
    }
    None
}

/// Return the text of the n-th positional `Ident` argument.
fn extract_nth_ident_arg(args: &SyntaxNode, n: usize) -> Option<String> {
    let mut pos = 0;
    for child in args.children() {
        match child.kind() {
            SyntaxKind::LeftParen
            | SyntaxKind::RightParen
            | SyntaxKind::Comma
            | SyntaxKind::Space
            | SyntaxKind::Named => continue,
            _ => {}
        }
        if pos == n {
            if child.kind() == SyntaxKind::Ident {
                return Some(child.text().to_string());
            }
            return None;
        }
        pos += 1;
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

// ---------------------------------------------------------------------------
// Wrapper-function detection
// ---------------------------------------------------------------------------

/// First-pass scan: collect all same-file function definitions that wrap `link()`.
pub fn collect_link_wrappers(root: &SyntaxNode) -> WrapperMap {
    let mut map = HashMap::new();
    collect_wrappers_from_node(root, &mut map);
    map
}

fn collect_wrappers_from_node(node: &SyntaxNode, map: &mut WrapperMap) {
    if node.kind() == SyntaxKind::LetBinding {
        try_register_wrapper(node, map);
    }
    for child in node.children() {
        collect_wrappers_from_node(child, map);
    }
}

fn try_register_wrapper(let_binding: &SyntaxNode, map: &mut WrapperMap) {
    // Case 1: Closure wrapper — #let f(x, y) = link(x, y)
    // The Closure is a direct child of LetBinding; the function name Ident
    // lives *inside* the Closure, not as a direct child of LetBinding.
    if let Some(closure) = let_binding
        .children()
        .find(|c| c.kind() == SyntaxKind::Closure)
    {
        register_closure_wrapper(closure, map);
        return;
    }

    // Case 2: Direct alias — #let mylink = link
    let idents: Vec<_> = let_binding
        .children()
        .filter(|c| c.kind() == SyntaxKind::Ident)
        .collect();

    if idents.len() < 2 {
        return;
    }

    let fn_name = idents[0].text().to_string();
    let has_link_alias = idents.iter().skip(1).any(|c| c.text() == LINK_IDENT_ID);
    if has_link_alias {
        map.insert(fn_name, 0);
    }
}

fn register_closure_wrapper(closure: &SyntaxNode, map: &mut WrapperMap) {
    // Function name is the first Ident child of the Closure
    let fn_name_node = match closure.children().find(|c| c.kind() == SyntaxKind::Ident) {
        Some(n) => n,
        None => return,
    };
    let fn_name = fn_name_node.text().to_string();

    let Some(params_node) = closure.children().find(|c| c.kind() == SyntaxKind::Params) else {
        return;
    };

    let param_names: Vec<String> = params_node
        .children()
        .filter(|c| c.kind() == SyntaxKind::Ident)
        .map(|c| c.text().to_string())
        .collect();

    let Some(link_call) = find_link_call(closure) else {
        return;
    };

    let Some(args) = link_call.children().find(|c| c.kind() == SyntaxKind::Args) else {
        return;
    };

    // Get first positional arg of link()
    let first_pos_arg = args.children().find(|c| {
        !matches!(
            c.kind(),
            SyntaxKind::LeftParen
                | SyntaxKind::RightParen
                | SyntaxKind::Comma
                | SyntaxKind::Space
                | SyntaxKind::Named
        )
    });

    let Some(first_arg) = first_pos_arg else {
        return;
    };

    // If first arg is a Str, the URL is hardcoded in the wrapper — skip
    if first_arg.kind() == SyntaxKind::Str {
        return;
    }

    // Must be an Ident matching a param
    if first_arg.kind() != SyntaxKind::Ident {
        return;
    }

    let arg_name = first_arg.text();
    let Some(param_idx) = param_names.iter().position(|n| n == arg_name.as_str()) else {
        return;
    };

    map.insert(fn_name, param_idx);
}

/// Find the first `FuncCall` to `link()` inside a subtree.
fn find_link_call(node: &SyntaxNode) -> Option<&SyntaxNode> {
    if node.kind() == SyntaxKind::FuncCall {
        let ident = node.children().find(|c| c.kind() == SyntaxKind::Ident)?;
        if ident.text() == LINK_IDENT_ID {
            return Some(node);
        }
    }
    for child in node.children() {
        if let Some(found) = find_link_call(child) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Let-bound URL variable collection
// ---------------------------------------------------------------------------

/// Collect file-scope `#let x = "./something.typ"` bindings.
///
/// Only bindings where the value is a string literal containing a `.typ`
/// path are tracked. Bindings inside closures or code blocks are skipped.
pub fn collect_url_bindings(root: &SyntaxNode) -> UrlBindingMap {
    let mut map = HashMap::new();
    collect_url_bindings_from_node(root, root, &mut map, false);
    map
}

fn collect_url_bindings_from_node(
    node: &SyntaxNode,
    root: &SyntaxNode,
    map: &mut UrlBindingMap,
    in_scope: bool,
) {
    // Only track file-scope bindings (not inside closures/code blocks)
    if in_scope {
        return;
    }

    if node.kind() == SyntaxKind::LetBinding {
        let children: Vec<_> = node.children().collect();
        // Find the binding name (first Ident) and the value (first Str after Eq)
        let name = children.iter().find(|c| c.kind() == SyntaxKind::Ident);
        let Some(name) = name else { return };
        let binding_name = name.text().to_string();

        // Look for a Str value — must be after the Eq token
        let mut after_eq = false;
        for child in &children {
            if child.kind() == SyntaxKind::Eq {
                after_eq = true;
                continue;
            }
            if after_eq && child.kind() == SyntaxKind::Str {
                let text = child.text().trim_matches('"').to_string();
                if is_relative_typ_link(&text)
                    && let Some(offset) = calculate_node_offset(root, child)
                {
                    let byte_range = offset..(offset + child.len());
                    map.insert(binding_name, (text, byte_range));
                }
                break;
            }
        }
        return; // Don't recurse into LetBinding children
    }

    // Stop descending into closures and code blocks
    if node.kind() == SyntaxKind::Closure || node.kind() == SyntaxKind::CodeBlock {
        return;
    }

    for child in node.children() {
        collect_url_bindings_from_node(child, root, map, in_scope);
    }
}

/// Parse a ModuleImport or ModuleInclude node into ImportInfo.
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
        assert!(!links[0].is_wrapper_call);
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

    // --- Wrapper function tests ---

    #[test]
    fn test_wrapper_direct_alias() {
        let source = Source::detached(
            r#"#let mylink = link
#mylink("./chapter2.typ")[text]"#,
        );
        let links = extract_links(&source);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "./chapter2.typ");
        assert!(links[0].is_wrapper_call);
    }

    #[test]
    fn test_wrapper_closure() {
        let source = Source::detached(
            r#"#let chapter-ref(path, title) = link(path, title)
#chapter-ref("./ch02.typ", [Chapter 2])"#,
        );
        let links = extract_links(&source);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].url, "./ch02.typ");
        assert!(links[0].is_wrapper_call);
    }

    #[test]
    fn test_wrapper_cross_file_not_detected() {
        let source = Source::detached(r#"#chapter-ref("./ch02.typ", [Ch 2])"#);
        let links = extract_links(&source);
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn test_wrapper_hardcoded_url_skipped() {
        // If link() is called with a hardcoded URL, nothing to rewrite at call site
        let source = Source::detached(
            r#"#let homepage(body) = link("https://example.com", body)
#homepage[text]"#,
        );
        let links = extract_links(&source);
        assert_eq!(links.len(), 0);
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

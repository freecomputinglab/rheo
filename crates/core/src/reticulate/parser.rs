use crate::reticulate::types::{ImportInfo, RheoValue, RheoVar};
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// Extract only package import path strings (those starting with '@') from
/// Typst source.
pub fn extract_package_imports(source: &Source) -> Vec<String> {
    let root = typst::syntax::parse(source.text());
    let mut out = Vec::new();
    collect_package_imports(&root, &root, &mut out);
    out
}

fn collect_package_imports(node: &SyntaxNode, root: &SyntaxNode, out: &mut Vec<String>) {
    if (node.kind() == SyntaxKind::ModuleImport || node.kind() == SyntaxKind::ModuleInclude)
        && let Some(info) = parse_import_node(node, root)
        && info.is_package
    {
        out.push(info.path);
    }
    for child in node.children() {
        collect_package_imports(child, root, out);
    }
}

/// Result of AST extraction.
pub struct ExtractedNodes {
    /// Top-level `#let rheo-<key> = "..."` bindings harvested from the source.
    pub rheo_vars: Vec<RheoVar>,
    /// All `<label>` names defined in the source (angle brackets stripped).
    pub user_labels: Vec<String>,
}

/// Extract rheo-* variables from Typst source.
///
/// Parses the source and traverses the AST to collect rheo-prefixed
/// let-bindings. Link extraction is deprecated — bundle compilation uses
/// Typst @ref for cross-file references.
pub fn extract_nodes(source: &Source) -> ExtractedNodes {
    let root = typst::syntax::parse(source.text());
    let rheo_vars = collect_rheo_vars(&root, source);
    let user_labels = collect_user_labels(source);
    ExtractedNodes {
        rheo_vars,
        user_labels,
    }
}

/// Walk the AST and return all `<label>` names defined in the source.
///
/// Strips the surrounding `<` and `>` to yield bare label name strings.
pub fn collect_user_labels(source: &Source) -> Vec<String> {
    let root = typst::syntax::parse(source.text());
    let mut labels = Vec::new();
    collect_labels_at(&root, &mut labels);
    labels
}

fn collect_labels_at(node: &SyntaxNode, out: &mut Vec<String>) {
    if node.kind() == SyntaxKind::Label {
        let text = node.leaf_text();
        let name = text.trim_start_matches('<').trim_end_matches('>');
        if !name.is_empty() {
            out.push(name.to_string());
        }
    }
    for child in node.children() {
        collect_labels_at(child, out);
    }
}

// ---------------------------------------------------------------------------
// rheo-* variable collection
// ---------------------------------------------------------------------------

/// Collect file-scope `#let rheo-<key> = "..."` bindings.
///
/// Only top-level bindings are tracked; bindings inside closures or code
/// blocks are skipped. For each `rheo-`-prefixed binding the RHS is recorded
/// as `Some(string)` when it is a string literal, otherwise `None` (the
/// consumer turns `None` into a validation error).
pub fn collect_rheo_vars(root: &SyntaxNode, source: &Source) -> Vec<RheoVar> {
    collect_rheo_vars_at(root, 0, source)
}

/// Recurse from `node`, whose first byte sits at `offset` in the source.
fn collect_rheo_vars_at(node: &SyntaxNode, offset: usize, source: &Source) -> Vec<RheoVar> {
    match node.kind() {
        // A file-scope let binding: harvest it if `rheo-`-prefixed, never recurse.
        SyntaxKind::LetBinding => parse_rheo_var(node, offset, source).into_iter().collect(),
        // Bindings inside closures or code blocks are not file-scope.
        SyntaxKind::Closure | SyntaxKind::CodeBlock => Vec::new(),
        _ => {
            let mut child_offset = offset;
            node.children()
                .flat_map(|child| {
                    let start = child_offset;
                    child_offset += child.len();
                    collect_rheo_vars_at(child, start, source)
                })
                .collect()
        }
    }
}

/// Parse a single `LetBinding` (starting at byte `offset`) into a `RheoVar` if
/// its name is `rheo-`-prefixed. The RHS is `Some(string)` for a string literal
/// and `None` for any other kind (the consumer turns `None` into an error).
fn parse_rheo_var(let_binding: &SyntaxNode, offset: usize, source: &Source) -> Option<RheoVar> {
    let name = let_binding
        .children()
        .find(|c| c.kind() == SyntaxKind::Ident)?;
    let key = name.leaf_text().strip_prefix("rheo-")?;

    // The value is the first meaningful node after `=` (skipping whitespace).
    let value = let_binding
        .children()
        .skip_while(|c| c.kind() != SyntaxKind::Eq)
        .skip(1)
        .find(|c| c.kind() != SyntaxKind::Space)
        .filter(|c| c.kind() == SyntaxKind::Str)
        .map(|c| RheoValue::Str(c.leaf_text().trim_matches('"').to_string()));

    let line = source
        .lines()
        .byte_to_line(offset)
        .map(|l| l + 1)
        .unwrap_or(1);

    Some(RheoVar {
        key: key.to_string(),
        value,
        line,
    })
}

/// Parse a ModuleImport or ModuleInclude node into ImportInfo.
fn parse_import_node(node: &SyntaxNode, root: &SyntaxNode) -> Option<ImportInfo> {
    // Find the first Str child — that is the path argument
    let str_node = node.children().find(|n| n.kind() == SyntaxKind::Str)?;

    let text = str_node.leaf_text();
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

    // --- rheo-* variable tests ---

    #[test]
    fn test_rheo_var_string() {
        let source = Source::detached(r#"#let rheo-feed-title = "Hello""#);
        let vars = collect_rheo_vars(&typst::syntax::parse(source.text()), &source);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].key, "feed-title");
        assert_eq!(vars[0].value, Some(RheoValue::Str("Hello".to_string())));
    }

    #[test]
    fn test_rheo_var_non_string_is_none() {
        let source = Source::detached(
            r#"#let rheo-count = 42
#let rheo-body = [x]"#,
        );
        let vars = collect_rheo_vars(&typst::syntax::parse(source.text()), &source);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].key, "count");
        assert_eq!(vars[0].value, None);
        assert_eq!(vars[1].key, "body");
        assert_eq!(vars[1].value, None);
    }

    #[test]
    fn test_rheo_var_in_block_ignored() {
        let source = Source::detached(
            r#"#{
  let rheo-inner = "nope"
}
#let f() = {
  let rheo-closure = "nope"
}"#,
        );
        let vars = collect_rheo_vars(&typst::syntax::parse(source.text()), &source);
        assert_eq!(vars.len(), 0);
    }

    #[test]
    fn test_rheo_vars_multiple_and_normal_skipped() {
        let source = Source::detached(
            r#"#let foo = "x"
#let rheo-feed-title = "Title"
#let rheo-feed-updated = "2025-01-15T00:00:00Z""#,
        );
        let vars = collect_rheo_vars(&typst::syntax::parse(source.text()), &source);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].key, "feed-title");
        assert_eq!(vars[0].value, Some(RheoValue::Str("Title".to_string())));
        assert_eq!(vars[1].key, "feed-updated");
        assert_eq!(
            vars[1].value,
            Some(RheoValue::Str("2025-01-15T00:00:00Z".to_string()))
        );
    }

    #[test]
    fn test_rheo_var_line_number() {
        let source = Source::detached(
            r#"Some text
#let rheo-feed-title = "Hello""#,
        );
        let vars = collect_rheo_vars(&typst::syntax::parse(source.text()), &source);
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0].line, 2);
    }

    #[test]
    fn test_extract_package_imports() {
        let source = Source::detached(r#"#import "@preview/tablex:0.0.6": tablex"#);
        let imports = extract_package_imports(&source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0], "@preview/tablex:0.0.6");
    }

    #[test]
    fn test_extract_package_imports_multiple() {
        let source = Source::detached(
            r#"#import "@preview/foo:1.0.0": *
#import "./local.typ": utils
#import "@preview/bar:2.0.0": bar"#,
        );
        let imports = extract_package_imports(&source);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0], "@preview/foo:1.0.0");
        assert_eq!(imports[1], "@preview/bar:2.0.0");
    }

    #[test]
    fn test_collect_user_labels() {
        let source = Source::detached(
            r#"= Introduction <intro>

Some text. <fig:chart>

#figure([], caption: [Chart]) <fig:chart>

== Section <sec-one>"#,
        );
        let mut labels = collect_user_labels(&source);
        labels.sort();
        assert_eq!(labels, vec!["fig:chart", "fig:chart", "intro", "sec-one"]);
    }

    #[test]
    fn test_collect_user_labels_empty() {
        let source = Source::detached("= No labels here\n\nJust text.");
        let labels = collect_user_labels(&source);
        assert!(labels.is_empty());
    }

    #[test]
    fn test_extract_nodes_rheo_vars() {
        let source = Source::detached(
            r#"#let rheo-feed-title = "Title"
#let rheo-feed-updated = "2025-01-15T00:00:00Z""#,
        );
        let extracted = extract_nodes(&source);
        assert_eq!(extracted.rheo_vars.len(), 2);
        assert_eq!(extracted.rheo_vars[0].key, "feed-title");
        assert_eq!(extracted.rheo_vars[1].key, "feed-updated");
    }
}

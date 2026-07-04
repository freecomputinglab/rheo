use crate::reticulate::types::{DocumentDate, ImportInfo, RheoValue, RheoVar};
use typst::syntax::{Source, SyntaxKind, SyntaxNode, ast};

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
    /// Parsed `#set document(date: datetime(...))` timestamp, if present.
    pub document_date: Option<DocumentDate>,
}

/// Extract rheo-* variables from Typst source.
///
/// Parses the source and traverses the AST to collect rheo-prefixed
/// let-bindings, the document date, and user-defined labels.
pub fn extract_nodes(source: &Source) -> ExtractedNodes {
    let root = typst::syntax::parse(source.text());
    let rheo_vars = collect_rheo_vars(&root, source);
    let user_labels = collect_user_labels(source);
    let document_date = DocumentDate::from_syntax(&root);
    ExtractedNodes {
        rheo_vars,
        user_labels,
        document_date,
    }
}

/// A value that can be located and decoded from a parsed Typst syntax tree.
///
/// Implement this to harvest one element of the core Typst syntax during the
/// canonical parse and thread it downstream — the same shape used for `rheo-*`
/// variables. An extractor yields at most one value; `None` means the element is
/// absent or could not be decoded.
pub trait FromSyntax: Sized {
    /// Locate and parse this value from the document `root`.
    fn from_syntax(root: &SyntaxNode) -> Option<Self>;
}

impl FromSyntax for DocumentDate {
    /// Walk the AST for a `set` rule targeting `document` whose `date:` argument is
    /// a `datetime(year: …, month: …, day: …[, hour: …, minute: …, second: …])`
    /// call. When no time components are present the time defaults to 00:00:00 UTC.
    ///
    /// Yields `None` when there is no `#set document`, no `date:` argument, the date
    /// is `none`/`auto`/`datetime.today()`, or the datetime is malformed/partial
    /// (missing year, month, or day, or an out-of-range value).
    fn from_syntax(root: &SyntaxNode) -> Option<Self> {
        if let Some(set_rule) = root.cast::<ast::SetRule>()
            && let ast::Expr::Ident(target) = set_rule.target()
            && target.as_str() == "document"
            && let Some(date) = Self::from_document_args(set_rule.args())
        {
            return Some(date);
        }
        root.children().find_map(Self::from_syntax)
    }
}

impl DocumentDate {
    /// Build a timestamp from a `#set document(...)` argument list, if it carries a
    /// `date: datetime(...)` argument.
    fn from_document_args(args: ast::Args) -> Option<Self> {
        use chrono::{TimeZone, Utc};

        // The `date:` named argument's value must be a `datetime(...)` call.
        let date_expr = args.items().find_map(|item| match item {
            ast::Arg::Named(named) if named.name().as_str() == "date" => Some(named.expr()),
            _ => None,
        })?;
        let ast::Expr::FuncCall(call) = date_expr else {
            return None;
        };
        let ast::Expr::Ident(callee) = call.callee() else {
            return None;
        };
        if callee.as_str() != "datetime" {
            return None;
        }

        let year = Self::named_int(call.args(), "year")?;
        let month = Self::named_int(call.args(), "month")?;
        let day = Self::named_int(call.args(), "day")?;
        let hour = Self::named_int(call.args(), "hour").unwrap_or(0);
        let minute = Self::named_int(call.args(), "minute").unwrap_or(0);
        let second = Self::named_int(call.args(), "second").unwrap_or(0);

        Utc.with_ymd_and_hms(
            i32::try_from(year).ok()?,
            u32::try_from(month).ok()?,
            u32::try_from(day).ok()?,
            u32::try_from(hour).ok()?,
            u32::try_from(minute).ok()?,
            u32::try_from(second).ok()?,
        )
        .single()
        .map(DocumentDate)
    }

    /// Read the integer value of a named argument (e.g. `year: 2025`).
    fn named_int(args: ast::Args, name: &str) -> Option<i64> {
        args.items().find_map(|item| match item {
            ast::Arg::Named(named) if named.name().as_str() == name => match named.expr() {
                ast::Expr::Int(int) => Some(int.get()),
                _ => None,
            },
            _ => None,
        })
    }
}

/// Walk the AST and return all `<label>` names **defined** in the source.
///
/// Only collects labels that appear in markup context (attached to content).
/// Labels inside function call arguments (`#link(<label>)`) are references,
/// not definitions, and are excluded.
///
/// Strips the surrounding `<` and `>` to yield bare label name strings.
pub fn collect_user_labels(source: &Source) -> Vec<String> {
    let root = typst::syntax::parse(source.text());
    let mut labels = Vec::new();
    collect_labels_at(&root, &mut labels, false);
    labels
}

fn collect_labels_at(node: &SyntaxNode, out: &mut Vec<String>, in_code: bool) {
    match node.kind() {
        SyntaxKind::Label if !in_code => {
            let text = node.leaf_text();
            let name = text.trim_start_matches('<').trim_end_matches('>');
            if !name.is_empty() {
                out.push(name.to_string());
            }
        }
        // These node kinds introduce a code context where labels are references.
        SyntaxKind::Args | SyntaxKind::CodeBlock | SyntaxKind::Code => {
            for child in node.children() {
                collect_labels_at(child, out, true);
            }
            return;
        }
        _ => {}
    }
    for child in node.children() {
        collect_labels_at(child, out, in_code);
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
    // String and boolean literals are supported; any other RHS yields `None`.
    let value = let_binding
        .children()
        .skip_while(|c| c.kind() != SyntaxKind::Eq)
        .skip(1)
        .find(|c| c.kind() != SyntaxKind::Space)
        .and_then(|c| match c.kind() {
            SyntaxKind::Str => Some(RheoValue::Str(c.leaf_text().trim_matches('"').to_string())),
            SyntaxKind::Bool => Some(RheoValue::Bool(c.leaf_text() == "true")),
            _ => None,
        });

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
    fn test_rheo_var_bool() {
        let source = Source::detached(
            r#"#let rheo-feed-exclude = true
#let rheo-draft = false"#,
        );
        let vars = collect_rheo_vars(&typst::syntax::parse(source.text()), &source);
        assert_eq!(vars.len(), 2);
        assert_eq!(vars[0].key, "feed-exclude");
        assert_eq!(vars[0].value, Some(RheoValue::Bool(true)));
        assert_eq!(vars[1].key, "draft");
        assert_eq!(vars[1].value, Some(RheoValue::Bool(false)));
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

    // --- document date tests ---

    fn document_date(src: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        extract_nodes(&Source::detached(src))
            .document_date
            .map(|d| d.0)
    }

    #[test]
    fn test_document_date_date_only() {
        use chrono::{Datelike, Timelike};
        let date = document_date(r#"#set document(date: datetime(year: 2025, month: 1, day: 15))"#)
            .expect("date should parse");
        assert_eq!((date.year(), date.month(), date.day()), (2025, 1, 15));
        assert_eq!((date.hour(), date.minute(), date.second()), (0, 0, 0));
    }

    #[test]
    fn test_document_date_with_time() {
        use chrono::{Datelike, Timelike};
        let date = document_date(
            r#"#set document(date: datetime(year: 2025, month: 3, day: 9, hour: 14, minute: 30, second: 5))"#,
        )
        .expect("date should parse");
        assert_eq!((date.year(), date.month(), date.day()), (2025, 3, 9));
        assert_eq!((date.hour(), date.minute(), date.second()), (14, 30, 5));
    }

    #[test]
    fn test_document_date_none() {
        assert!(document_date(r#"#set document(date: none)"#).is_none());
    }

    #[test]
    fn test_document_date_auto() {
        assert!(document_date(r#"#set document(date: auto)"#).is_none());
    }

    #[test]
    fn test_document_date_absent() {
        assert!(document_date(r#"#set document(title: [No Date Here])"#).is_none());
    }

    #[test]
    fn test_document_date_partial_is_none() {
        // Missing `day` → cannot build a date.
        assert!(document_date(r#"#set document(date: datetime(year: 2025, month: 1))"#).is_none());
    }

    #[test]
    fn test_document_date_today_is_none() {
        // `datetime.today()` can't be resolved statically → None.
        assert!(document_date(r#"#set document(date: datetime.today())"#).is_none());
    }

    #[test]
    fn test_document_date_ignores_other_set_rules() {
        // A `#set page(...)` before the document rule must not confuse the walk.
        use chrono::Datelike;
        let date = document_date(
            r#"#set page(width: 10cm)
#set document(title: [Doc], date: datetime(year: 2024, month: 12, day: 31))"#,
        )
        .expect("date should parse");
        assert_eq!((date.year(), date.month(), date.day()), (2024, 12, 31));
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

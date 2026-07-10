//! Extraction of structured data from Typst syntax trees.
//!
//! Everything here is built on one abstraction: [`SyntaxSite`]. A `SyntaxSite`
//! says *what* to pull out of the tree — a label, a reference, a `rheo-*`
//! binding, the document date, a package import — while the shared walker owns
//! the *how*: one depth-first pass ([`walk_tree`]) that threads each node's byte
//! offset and markup/code context and visits every node exactly once.
//!
//! Two entry points consume that walker:
//!
//! * [`SyntaxSite::collect`] / [`SyntaxSite::first`] gather **one** site type,
//!   parsing and walking once per call — for standalone callers that need just
//!   labels, or just imports.
//! * [`extract_nodes`] gathers **all** the per-vertebra metadata (labels,
//!   `rheo-*` vars, document date) in a **single** parse and a **single**
//!   traversal, fanning each node out to every visitor. This is the hot path in
//!   spine building; the once-only guarantee is a design constraint, enforced by
//!   `extract_nodes_parses_and_traverses_once`.

use std::ops::Range;
use typst::syntax::{Source, SyntaxKind, SyntaxNode, ast};

// ===========================================================================
// The SyntaxSite abstraction
// ===========================================================================

/// Context threaded down a syntax-tree walk, derived centrally by
/// [`descend_ctx`] as the walker descends. Visitors read the flags they care
/// about; they never compute context themselves.
#[derive(Clone, Copy)]
pub struct WalkCtx {
    /// Inside code context (function arguments, code blocks) where a `<name>`
    /// label is a *reference* (`#link(<name>)`) rather than a *definition*.
    pub in_code: bool,
    /// At the top markup level, where a `#let rheo-*` binding is file-scope.
    /// Cleared on descent into a closure, code block, or another binding's RHS.
    pub file_scope: bool,
}

impl Default for WalkCtx {
    fn default() -> Self {
        // The document root is top-level markup: file scope, not code.
        WalkCtx {
            in_code: false,
            file_scope: true,
        }
    }
}

/// Derive the context for the *children* of a node of `kind`, given the node's
/// own context. Applied once per node before recursing, so a node is inspected
/// under its parent's context while its subtree sees the updated one — e.g. a
/// top-level `#let` is itself file-scope, but its RHS is not.
///
/// This is the single place descent semantics live, so every visitor in a walk
/// agrees on what "code context" and "file scope" mean.
fn descend_ctx(kind: SyntaxKind, ctx: WalkCtx) -> WalkCtx {
    use SyntaxKind::{Args, Closure, Code, CodeBlock, LetBinding};
    WalkCtx {
        // Function args and code blocks are code context: a `<name>` inside them
        // is a reference (`#link(<name>)`), not a label definition.
        in_code: ctx.in_code || matches!(kind, Args | CodeBlock | Code),
        // A `rheo-*` binding counts only at the top markup level. Inside a
        // closure, a code block, or a binding's own RHS, file scope is left.
        file_scope: ctx.file_scope && !matches!(kind, Closure | CodeBlock | LetBinding),
    }
}

/// An element locatable across a Typst syntax tree during a single depth-first,
/// offset-tracking walk.
///
/// Implement [`visit`](SyntaxSite::visit) to inspect a node and record any
/// matches; the shared [`walk_tree`] drives traversal, handing each node its
/// byte offset and centrally-derived [`WalkCtx`]. Descent and context are the
/// walker's job — a visitor only inspects the current node.
///
/// [`MAX_SITES`](SyntaxSite::MAX_SITES) bounds how many sites
/// [`collect`](SyntaxSite::collect) gathers before the walk halts. `None`
/// collects every occurrence (e.g. every label); `Some(1)` models a
/// "find the first" single-value extractor — use [`first`](SyntaxSite::first)
/// to get that lone value as an `Option` (as [`DocumentDate`] does).
pub trait SyntaxSite: Sized {
    /// Stop [`collect`](SyntaxSite::collect) once this many sites are found.
    /// `None` = unbounded.
    const MAX_SITES: Option<usize> = None;

    /// Inspect `node` (whose first byte is at `offset`) under `ctx` and push any
    /// matches to `out`. `source` is provided for extractors that need line
    /// numbers or wider context; most need only the node and its offset.
    fn visit(source: &Source, node: &SyntaxNode, offset: usize, ctx: WalkCtx, out: &mut Vec<Self>);

    /// Collect sites of this one type from `source` — a single parse and a
    /// single walk, up to [`MAX_SITES`](SyntaxSite::MAX_SITES).
    fn collect(source: &Source) -> Vec<Self> {
        let root = parse_source(source);
        let mut out = Vec::new();
        walk_tree(source, &root, 0, WalkCtx::default(), &mut |s, n, o, c| {
            Self::visit(s, n, o, c, &mut out);
            Self::MAX_SITES.is_none_or(|max| out.len() < max)
        });
        out
    }

    /// The first site in document order, if any. Pairs with `MAX_SITES = Some(1)`.
    fn first(source: &Source) -> Option<Self> {
        Self::collect(source).into_iter().next()
    }
}

/// Parse `source` into a syntax tree. The one parse seam in this module, so the
/// once-only guarantee can be observed in tests.
fn parse_source(source: &Source) -> SyntaxNode {
    #[cfg(test)]
    PARSE_COUNT.with(|c| c.set(c.get() + 1));
    typst::syntax::parse(source.text())
}

/// Depth-first walk from `node` (first byte at `offset`), invoking `visit` on
/// every node with its offset and centrally-derived context. Each node is
/// visited exactly once; returns early (yielding `false`) as soon as `visit`
/// returns `false`. This is the sole tree traversal in the module.
fn walk_tree<F>(
    source: &Source,
    node: &SyntaxNode,
    offset: usize,
    ctx: WalkCtx,
    visit: &mut F,
) -> bool
where
    F: FnMut(&Source, &SyntaxNode, usize, WalkCtx) -> bool,
{
    if !visit(source, node, offset, ctx) {
        return false;
    }
    // Children share one context, derived from this node's kind.
    let child_ctx = descend_ctx(node.kind(), ctx);
    let mut child_offset = offset;
    for child in node.children() {
        if !walk_tree(source, child, child_offset, child_ctx, visit) {
            return false;
        }
        child_offset += child.len();
    }
    true
}

/// Walk `root` once, fanning every node out to `visit`. Distinct from
/// [`SyntaxSite::collect`] in that a single traversal feeds several visitors —
/// the basis of [`extract_nodes`]' one-parse/one-walk guarantee.
fn walk_once(
    source: &Source,
    root: &SyntaxNode,
    mut visit: impl FnMut(&Source, &SyntaxNode, usize, WalkCtx),
) {
    #[cfg(test)]
    WALK_COUNT.with(|c| c.set(c.get() + 1));
    walk_tree(source, root, 0, WalkCtx::default(), &mut |s, n, o, c| {
        visit(s, n, o, c);
        true
    });
}

// ===========================================================================
// Extractor: `<label>` definitions and `@ref` / `#link(<label>)` references
// ===========================================================================

/// Whether a [`LabelSite`] defines a label or references one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelRole {
    /// A markup-context `<name>` — defines a label on the surrounding content.
    Definition,
    /// A `@name` marker or a code-context `<name>` — references a label.
    Reference,
}

/// A label occurrence with the byte range of the token to rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelSite {
    /// Bare label name: the `<name>` brackets or a leading `@` stripped.
    pub name: String,
    /// Whether this site defines or references a label.
    pub role: LabelRole,
    /// Byte range of the rewriteable token in the source — the full `<name>`
    /// for a label, or the `@name` marker for a reference (a trailing `[..]`
    /// supplement is excluded).
    pub range: Range<usize>,
}

impl SyntaxSite for LabelSite {
    fn visit(
        _source: &Source,
        node: &SyntaxNode,
        offset: usize,
        ctx: WalkCtx,
        out: &mut Vec<Self>,
    ) {
        match node.kind() {
            // A `<name>` label token (leaf). Markup context → definition; code
            // context (function args) → reference, e.g. `#link(<name>)`.
            SyntaxKind::Label => {
                let text = node.leaf_text();
                let name = text.trim_start_matches('<').trim_end_matches('>');
                if !name.is_empty() {
                    let role = if ctx.in_code {
                        LabelRole::Reference
                    } else {
                        LabelRole::Definition
                    };
                    out.push(LabelSite {
                        name: name.to_string(),
                        role,
                        range: offset..offset + node.len(),
                    });
                }
            }
            // An `@name` reference marker (leaf). Always a reference; any `[..]`
            // supplement is a sibling node and is excluded from the range.
            SyntaxKind::RefMarker => {
                let name = node.leaf_text().trim_start_matches('@');
                if !name.is_empty() {
                    out.push(LabelSite {
                        name: name.to_string(),
                        role: LabelRole::Reference,
                        range: offset..offset + node.len(),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Label definition and reference sites in a source, partitioned by role.
///
/// `definitions` are markup-context `<name>` labels (attached to content).
/// `references` are `@name` markers plus code-context `<name>` labels such as
/// `#link(<name>)` / `#ref(<name>)`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LabelSites {
    pub definitions: Vec<LabelSite>,
    pub references: Vec<LabelSite>,
}

/// Collect label sites from `source` and partition them by role.
///
/// A view over `LabelSite::collect`: ranges are exact byte offsets into
/// `source`, suitable for splicing.
pub fn extract_label_sites(source: &Source) -> LabelSites {
    let mut sites = LabelSites::default();
    for site in LabelSite::collect(source) {
        match site.role {
            LabelRole::Definition => sites.definitions.push(site),
            LabelRole::Reference => sites.references.push(site),
        }
    }
    sites
}

/// Return all `<label>` names **defined** in the source (markup context only).
///
/// Labels inside function-call arguments (`#link(<label>)`) are references, not
/// definitions, and are excluded. Surrounding `<`/`>` are stripped.
pub fn collect_user_labels(source: &Source) -> Vec<String> {
    extract_label_sites(source)
        .definitions
        .into_iter()
        .map(|site| site.name)
        .collect()
}

// ===========================================================================
// Extractor: `#set document(date: datetime(...))`
// ===========================================================================

/// The `#set document(date: datetime(...))` timestamp harvested from a spine
/// vertebra during the canonical Typst parse, threaded into downstream features
/// (the HTML Atom feed).
///
/// A [`SyntaxSite`] capped at one site: the first such rule in the tree, read
/// via `DocumentDate::first(source)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentDate(pub chrono::DateTime<chrono::Utc>);

impl SyntaxSite for DocumentDate {
    const MAX_SITES: Option<usize> = Some(1);

    /// Match a `set` rule targeting `document` whose `date:` argument is a
    /// `datetime(year:, month:, day:[, hour:, minute:, second:])` call; absent
    /// time components default to 00:00:00 UTC. Records nothing when the date is
    /// `none`/`auto`/`datetime.today()` or malformed/partial.
    fn visit(
        _source: &Source,
        node: &SyntaxNode,
        _offset: usize,
        _ctx: WalkCtx,
        out: &mut Vec<Self>,
    ) {
        if let Some(set_rule) = node.cast::<ast::SetRule>()
            && let ast::Expr::Ident(target) = set_rule.target()
            && target.as_str() == "document"
            && let Some(date) = Self::from_document_args(set_rule.args())
        {
            out.push(date);
        }
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

// ===========================================================================
// Extractor: top-level `#let rheo-<key> = "..."` bindings
// ===========================================================================

/// A value bound to a `rheo-*` variable. String literals and booleans are
/// supported; the enum exists so further kinds (e.g. datetimes) can be added
/// without changing every consumer's signature.
#[derive(Debug, Clone, PartialEq)]
pub enum RheoValue {
    /// A string literal RHS.
    Str(String),
    /// A boolean literal RHS (`true`/`false`).
    Bool(bool),
}

impl RheoValue {
    /// The inner string if this is a [`RheoValue::Str`], else `None`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            RheoValue::Str(s) => Some(s),
            RheoValue::Bool(_) => None,
        }
    }

    /// The inner bool if this is a [`RheoValue::Bool`], else `None`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            RheoValue::Bool(b) => Some(*b),
            RheoValue::Str(_) => None,
        }
    }
}

/// A top-level `#let rheo-<key> = "..."` binding harvested from a spine
/// vertebra during the canonical Typst parse.
#[derive(Debug, Clone, PartialEq)]
pub struct RheoVar {
    /// The let-binding name with the leading `rheo-` prefix stripped
    /// (e.g. `rheo-feed-title` → `feed-title`).
    pub key: String,

    /// `Some(value)` when the RHS is a supported kind; `None` when it is any
    /// other kind. The consumer turns `None` into a validation error.
    pub value: Option<RheoValue>,

    /// 1-based source line of the binding, for error messages.
    pub line: usize,
}

impl SyntaxSite for RheoVar {
    /// Harvest file-scope `#let rheo-<key> = ...` bindings. `ctx.file_scope` (set
    /// by [`child_ctx`]) means the binding is at the top markup level, not nested
    /// in a closure, code block, or another binding's RHS.
    fn visit(source: &Source, node: &SyntaxNode, offset: usize, ctx: WalkCtx, out: &mut Vec<Self>) {
        if ctx.file_scope && node.kind() == SyntaxKind::LetBinding {
            out.extend(parse_rheo_var(node, offset, source));
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

// ===========================================================================
// Extractor: `#import`/`#include` package paths
// ===========================================================================

/// A path string extracted from an `#import`/`#include` statement.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The raw path string (e.g. `./utils.typ` or `@preview/foo:0.1.0`).
    pub path: String,
    /// Byte range of the path string (not the whole statement).
    pub byte_range: Range<usize>,
    /// `true` if the path starts with `@` (a package import).
    pub is_package: bool,
}

impl SyntaxSite for ImportInfo {
    fn visit(
        _source: &Source,
        node: &SyntaxNode,
        offset: usize,
        _ctx: WalkCtx,
        out: &mut Vec<Self>,
    ) {
        if matches!(
            node.kind(),
            SyntaxKind::ModuleImport | SyntaxKind::ModuleInclude
        ) && let Some(info) = parse_import_node(node, offset)
        {
            out.push(info);
        }
    }
}

/// Parse a `ModuleImport`/`ModuleInclude` (starting at byte `node_offset`) into
/// `ImportInfo`, reading the first `Str` child as the path and deriving its byte
/// range from the walker-supplied offset.
fn parse_import_node(node: &SyntaxNode, node_offset: usize) -> Option<ImportInfo> {
    let mut offset = node_offset;
    for child in node.children() {
        if child.kind() == SyntaxKind::Str {
            let path = child.leaf_text().trim_matches('"').to_string();
            let byte_range = offset..offset + child.len();
            return Some(ImportInfo {
                is_package: path.starts_with('@'),
                path,
                byte_range,
            });
        }
        offset += child.len();
    }
    None
}

/// Extract package import paths (those starting with `@`) from Typst source.
pub fn extract_package_imports(source: &Source) -> Vec<String> {
    ImportInfo::collect(source)
        .into_iter()
        .filter(|info| info.is_package)
        .map(|info| info.path)
        .collect()
}

// ===========================================================================
// Aggregate extraction: one parse, one traversal
// ===========================================================================

/// Everything harvested from a vertebra's source in the canonical parse.
pub struct ExtractedNodes {
    /// Top-level `#let rheo-<key> = "..."` bindings harvested from the source.
    pub rheo_vars: Vec<RheoVar>,
    /// All `<label>` names defined in the source (angle brackets stripped).
    pub user_labels: Vec<String>,
    /// Parsed `#set document(date: datetime(...))` timestamp, if present.
    pub document_date: Option<DocumentDate>,
}

/// Harvest labels, `rheo-*` bindings, and the document date from `source` in a
/// **single** parse and a **single** traversal, fanning each node out to every
/// visitor.
///
/// Parsing and traversing exactly once is a design constraint (parse is the
/// costly step; this runs per vertebra during spine building). Enforced by
/// `extract_nodes_parses_and_traverses_once`.
pub fn extract_nodes(source: &Source) -> ExtractedNodes {
    let root = parse_source(source);
    let mut labels = Vec::new();
    let mut rheo_vars = Vec::new();
    let mut dates = Vec::new();
    walk_once(source, &root, |s, n, o, c| {
        LabelSite::visit(s, n, o, c, &mut labels);
        RheoVar::visit(s, n, o, c, &mut rheo_vars);
        DocumentDate::visit(s, n, o, c, &mut dates);
    });
    ExtractedNodes {
        rheo_vars,
        user_labels: labels
            .into_iter()
            .filter(|l| l.role == LabelRole::Definition)
            .map(|l| l.name)
            .collect(),
        document_date: dates.into_iter().next(),
    }
}

// Instrumentation for the once-only guarantee. Thread-local so parallel test
// threads don't interfere; each guarantee test resets before measuring.
#[cfg(test)]
thread_local! {
    static PARSE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static WALK_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::Source;

    // --- single parse / single traversal guarantee ---

    fn count_nodes(node: &SyntaxNode) -> usize {
        1 + node.children().map(|c| count_nodes(&c)).sum::<usize>()
    }

    #[test]
    fn walk_tree_visits_each_node_once() {
        let source = Source::detached("= H <a>\n\n#let rheo-x = \"y\"\n\nSee @a.");
        let root = typst::syntax::parse(source.text());
        let expected = count_nodes(&root);

        let mut visited = 0usize;
        walk_tree(&source, &root, 0, WalkCtx::default(), &mut |_, _, _, _| {
            visited += 1;
            true
        });
        assert_eq!(visited, expected, "walk must visit each node exactly once");
    }

    #[test]
    fn extract_nodes_parses_and_traverses_once() {
        // Source exercising every visitor, so no extractor can justify a second pass.
        let source = Source::detached(
            r#"#set document(date: datetime(year: 2025, month: 1, day: 2))
#let rheo-feed-title = "T"
= Heading <h>
See @h and #link(<h>)[here]."#,
        );
        PARSE_COUNT.with(|c| c.set(0));
        WALK_COUNT.with(|c| c.set(0));

        let _ = extract_nodes(&source);

        assert_eq!(
            PARSE_COUNT.with(|c| c.get()),
            1,
            "extract_nodes must parse the source exactly once"
        );
        assert_eq!(
            WALK_COUNT.with(|c| c.get()),
            1,
            "extract_nodes must traverse the tree exactly once"
        );
    }

    // --- rheo-* variable tests ---

    #[test]
    fn test_rheo_var_string() {
        let source = Source::detached(r#"#let rheo-feed-title = "Hello""#);
        let vars = RheoVar::collect(&source);
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
        let vars = RheoVar::collect(&source);
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
        let vars = RheoVar::collect(&source);
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
        let vars = RheoVar::collect(&source);
        assert_eq!(vars.len(), 0);
    }

    #[test]
    fn test_rheo_vars_multiple_and_normal_skipped() {
        let source = Source::detached(
            r#"#let foo = "x"
#let rheo-feed-title = "Title"
#let rheo-feed-updated = "2025-01-15T00:00:00Z""#,
        );
        let vars = RheoVar::collect(&source);
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
        let vars = RheoVar::collect(&source);
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

    // --- label site extraction tests ---

    /// Assert that each site's recorded range slices back to `expected` text.
    fn assert_sites_slice(src: &str, sites: &[LabelSite], expected: &[(&str, &str)]) {
        assert_eq!(
            sites.len(),
            expected.len(),
            "site count mismatch: {sites:?}"
        );
        for (site, (name, text)) in sites.iter().zip(expected) {
            assert_eq!(&site.name, name);
            assert_eq!(&src[site.range.clone()], *text);
        }
    }

    #[test]
    fn test_sites_heading_definition() {
        let src = "= Intro <intro>";
        let sites = extract_label_sites(&Source::detached(src));
        assert_sites_slice(src, &sites.definitions, &[("intro", "<intro>")]);
        assert!(sites.references.is_empty());
    }

    #[test]
    fn test_sites_figure_label_definition() {
        let src = "#figure([], caption: [c]) <fig:chart>";
        let sites = extract_label_sites(&Source::detached(src));
        assert_sites_slice(src, &sites.definitions, &[("fig:chart", "<fig:chart>")]);
        assert!(sites.references.is_empty());
    }

    #[test]
    fn test_sites_ref_marker_is_reference() {
        let src = "See @intro for more.";
        let sites = extract_label_sites(&Source::detached(src));
        assert!(sites.definitions.is_empty());
        assert_sites_slice(src, &sites.references, &[("intro", "@intro")]);
    }

    #[test]
    fn test_sites_ref_with_supplement_excludes_supplement() {
        // The `[p.9]` supplement is a sibling node; the range covers only `@key`.
        let src = "Citation @key[p.9] here.";
        let sites = extract_label_sites(&Source::detached(src));
        assert_sites_slice(src, &sites.references, &[("key", "@key")]);
    }

    #[test]
    fn test_sites_link_and_ref_calls_are_references() {
        let src = "#link(<a>)[text] and #ref(<b>)";
        let sites = extract_label_sites(&Source::detached(src));
        assert!(sites.definitions.is_empty());
        assert_sites_slice(src, &sites.references, &[("a", "<a>"), ("b", "<b>")]);
    }

    #[test]
    fn test_sites_definition_and_reference_classified() {
        let src = "== Section <sec>\n\nBack to @sec.";
        let sites = extract_label_sites(&Source::detached(src));
        assert_sites_slice(src, &sites.definitions, &[("sec", "<sec>")]);
        assert_sites_slice(src, &sites.references, &[("sec", "@sec")]);
    }

    #[test]
    fn test_syntaxsite_collect_tags_roles() {
        // The SyntaxSite trait's collect() returns every site in one flat list,
        // each tagged with its role; extract_label_sites is the partition view.
        let src = "= A <a>\n\n#link(<a>)[x] and @a.";
        let roles: Vec<LabelRole> = LabelSite::collect(&Source::detached(src))
            .into_iter()
            .map(|s| s.role)
            .collect();
        assert_eq!(
            roles,
            vec![
                LabelRole::Definition,
                LabelRole::Reference,
                LabelRole::Reference
            ]
        );
    }

    #[test]
    fn test_syntaxsite_first_returns_leading_site() {
        // first() pairs with the MAX_SITES cap: the leading site in document order.
        let src = "= A <a>\n\n== B <b>";
        let first = LabelSite::first(&Source::detached(src)).expect("a label");
        assert_eq!(first.name, "a");
    }

    #[test]
    fn test_collect_user_labels_matches_definitions() {
        // collect_user_labels is a thin wrapper over extract_label_sites definitions.
        let src = "= A <a>\n\n#link(<a>)[x]\n\n== B <b>";
        let source = Source::detached(src);
        let mut labels = collect_user_labels(&source);
        labels.sort();
        assert_eq!(labels, vec!["a", "b"]);
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

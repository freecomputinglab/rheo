//! Extractor: top-level `#let rheo-<key> = "..."` bindings.

use super::{SyntaxSite, WalkCtx};
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

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
    /// by the walker) means the binding is at the top markup level, not nested in
    /// a closure, code block, or another binding's RHS.
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

#[cfg(test)]
mod tests {
    use super::*;

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
}

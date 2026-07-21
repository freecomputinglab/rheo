//! Extractor: every named argument of `#set document(...)`.

use super::{SyntaxSite, WalkCtx};
use crate::util::typst_literal::TypstLiteral;
use typst::syntax::{Source, SyntaxKind, SyntaxNode, ast, ast::AstNode};

/// A value harvested from a `#set document(...)` named argument.
///
/// Covers the literal shapes a static AST walk can faithfully round-trip back
/// into a Typst value: strings (including bracket-content, flattened to plain
/// text), booleans, integers, floats, and arrays thereof. Non-literal arguments
/// (e.g. `date: datetime(...)`, which is a function call) are not representable
/// here and are dropped by [`MetaValue::from_expr`]; the document date has its
/// own dedicated extractor, [`DocumentDate`](super::DocumentDate).
#[derive(Debug, Clone, PartialEq)]
pub enum MetaValue {
    /// A string literal, or the plain text of a bracket-content value.
    Str(String),
    /// A boolean literal (`true`/`false`).
    Bool(bool),
    /// An integer literal.
    Int(i64),
    /// A floating-point literal.
    Float(f64),
    /// An array literal, e.g. `("DiH", "MiT")` for `keywords`.
    Array(Vec<MetaValue>),
}

impl MetaValue {
    /// The inner string if this is a [`MetaValue::Str`], else `None`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            MetaValue::Str(s) => Some(s),
            _ => None,
        }
    }

    /// Convert one argument expression into a [`MetaValue`], returning `None` for
    /// any expression kind that is not a faithfully-representable literal (e.g. a
    /// `datetime(...)` call or an identifier).
    fn from_expr(expr: ast::Expr) -> Option<Self> {
        match expr {
            ast::Expr::Str(s) => Some(MetaValue::Str(s.get().to_string())),
            ast::Expr::Bool(b) => Some(MetaValue::Bool(b.get())),
            ast::Expr::Int(i) => Some(MetaValue::Int(i.get())),
            ast::Expr::Float(f) => Some(MetaValue::Float(f.get())),
            // Bracket-content (`title: [My Title]`) flattens to its plain text,
            // dropping markup markers so a spine/feed title is clean text.
            ast::Expr::ContentBlock(c) => {
                Some(MetaValue::Str(markup_plain_text(c.body().to_untyped())))
            }
            ast::Expr::Array(a) => Some(MetaValue::Array(
                a.items()
                    .filter_map(|item| match item {
                        ast::ArrayItem::Pos(e) => MetaValue::from_expr(e),
                        _ => None,
                    })
                    .collect(),
            )),
            _ => None,
        }
    }

    /// Render this value as a [`TypstLiteral`], for injection into the spine's
    /// `metadata` dict.
    pub fn to_literal(&self) -> TypstLiteral {
        match self {
            MetaValue::Str(s) => TypstLiteral::str(s.as_str()),
            MetaValue::Bool(b) => TypstLiteral::bool(*b),
            MetaValue::Int(i) => TypstLiteral::Int(*i),
            MetaValue::Float(f) => TypstLiteral::Float(*f),
            MetaValue::Array(items) => {
                TypstLiteral::Array(items.iter().map(MetaValue::to_literal).collect())
            }
        }
    }
}

/// All named arguments of the first `#set document(...)` rule in a vertebra,
/// captured generically as `(name, value)` pairs in source order.
///
/// A [`SyntaxSite`] capped at one site: the first such rule in the tree, read
/// via `DocumentMetadata::first(source)`. Only literal-valued arguments are
/// retained (see [`MetaValue`]); an argument whose value cannot be represented
/// as a literal is silently skipped rather than erroring, so an ordinary
/// `date: datetime(...)` argument does not abort the harvest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentMetadata(pub Vec<(String, MetaValue)>);

impl SyntaxSite for DocumentMetadata {
    const MAX_SITES: Option<usize> = Some(1);

    /// Match a `set` rule targeting `document` and capture each of its named
    /// arguments whose value is a representable literal.
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
        {
            let fields = set_rule
                .args()
                .items()
                .filter_map(|item| match item {
                    ast::Arg::Named(named) => MetaValue::from_expr(named.expr())
                        .map(|v| (named.name().as_str().to_string(), v)),
                    _ => None,
                })
                .collect();
            out.push(DocumentMetadata(fields));
        }
    }
}

impl DocumentMetadata {
    /// The value of a named argument (e.g. `title`), if present.
    pub fn get(&self, name: &str) -> Option<&MetaValue> {
        self.0.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    /// Serialize the captured metadata to a [`TypstLiteral`] dictionary for the
    /// spine's `metadata` field. Empty metadata serializes to `(:)`.
    pub fn to_literal(&self) -> TypstLiteral {
        TypstLiteral::Dict(
            self.0
                .iter()
                .map(|(k, v)| (k.clone(), v.to_literal()))
                .collect(),
        )
    }
}

/// Flatten a markup subtree to its plain text: concatenate every `Text`/`Space`
/// leaf, dropping markup markers (emphasis underscores, `#strong[...]`, brackets)
/// so `[Good news - #emph[Severance]]` becomes `Good news - Severance`.
fn markup_plain_text(node: &SyntaxNode) -> String {
    let mut out = String::new();
    collect_text(node, &mut out);
    out.trim().to_string()
}

/// Append the text of every `Text`/`Space` leaf under `node`, in order.
fn collect_text(node: &SyntaxNode, out: &mut String) {
    match node.kind() {
        SyntaxKind::Text | SyntaxKind::Space => out.push_str(node.leaf_text()),
        _ => {
            for child in node.children() {
                collect_text(child, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(src: &str) -> DocumentMetadata {
        DocumentMetadata::first(&Source::detached(src)).unwrap_or_default()
    }

    #[test]
    fn test_string_title() {
        let m = metadata(r#"#set document(title: "My Post")"#);
        assert_eq!(m.get("title").and_then(MetaValue::as_str), Some("My Post"));
    }

    #[test]
    fn test_bracket_title_flattens_to_plain_text() {
        let m = metadata(r#"#set document(title: [Good news - #emph[Severance]])"#);
        assert_eq!(
            m.get("title").and_then(MetaValue::as_str),
            Some("Good news - Severance")
        );
    }

    #[test]
    fn test_keywords_round_trip_as_array_of_strings() {
        let m = metadata(r#"#set document(title: [X], keywords: ("DiH", "MiT"))"#);
        assert_eq!(
            m.get("keywords"),
            Some(&MetaValue::Array(vec![
                MetaValue::Str("DiH".into()),
                MetaValue::Str("MiT".into()),
            ]))
        );
        // Serializes to a Typst array literal.
        assert_eq!(
            m.get("keywords").unwrap().to_literal().serialize(),
            r#"("DiH", "MiT",)"#
        );
    }

    #[test]
    fn test_single_keyword_trailing_comma() {
        let m = metadata(r#"#set document(keywords: ("DiH",))"#);
        assert_eq!(
            m.get("keywords"),
            Some(&MetaValue::Array(vec![MetaValue::Str("DiH".into())]))
        );
    }

    #[test]
    fn test_int_and_bool_and_float_args() {
        let m = metadata(r#"#set document(reading-time: 5, draft: true, ratio: 1.5)"#);
        assert_eq!(m.get("reading-time"), Some(&MetaValue::Int(5)));
        assert_eq!(m.get("draft"), Some(&MetaValue::Bool(true)));
        assert_eq!(m.get("ratio"), Some(&MetaValue::Float(1.5)));
    }

    #[test]
    fn test_datetime_arg_skipped_not_errored() {
        // `date: datetime(...)` is a function call, not a literal → dropped, but
        // the other representable args are still harvested.
        let m = metadata(
            r#"#set document(title: [T], date: datetime(year: 2025, month: 1, day: 2), keywords: ("A",))"#,
        );
        assert!(m.get("date").is_none());
        assert_eq!(m.get("title").and_then(MetaValue::as_str), Some("T"));
        assert!(m.get("keywords").is_some());
    }

    #[test]
    fn test_no_document_rule_is_empty() {
        let m = metadata("= Heading\n\nBody text.");
        assert!(m.0.is_empty());
    }

    #[test]
    fn test_first_document_rule_only() {
        // MAX_SITES = 1: only the first `#set document(...)` is captured.
        let m = metadata(
            r#"#set document(title: [First])
#set document(title: [Second])"#,
        );
        assert_eq!(m.get("title").and_then(MetaValue::as_str), Some("First"));
    }

    #[test]
    fn test_ignores_other_set_rules() {
        let m = metadata(
            r#"#set page(width: 10cm)
#set document(title: [Doc], keywords: ("K",))"#,
        );
        assert_eq!(m.get("title").and_then(MetaValue::as_str), Some("Doc"));
    }

    #[test]
    fn test_empty_metadata_serializes_to_empty_dict() {
        let m = metadata("= Heading");
        assert_eq!(m.to_literal().serialize(), "(:)");
    }
}

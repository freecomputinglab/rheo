//! Extractor: every named argument of `#set document(...)`.

use super::{SyntaxSite, WalkCtx};
use crate::util::typst_literal::TypstLiteral;
use typst::syntax::{Source, SyntaxKind, SyntaxNode, ast, ast::AstNode};

/// A value harvested from a `#set document(...)` named argument.
///
/// Typst's `document` element is not extensible: a `#set document(...)` rule only
/// accepts its defined parameters — `title` (content/str), `author` and
/// `keywords` (str or array of str), and `date` (a `datetime(...)` call, skipped
/// here as a non-literal and handled by [`DocumentDate`](super::DocumentDate)).
/// So the only literal shapes reachable through a compilable rule are strings
/// (including bracket-content, flattened to plain text) and arrays of them.
#[derive(Debug, Clone, PartialEq)]
pub enum MetaValue {
    /// A string literal, or the plain text of a bracket-content value.
    Str(String),
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
            // Bracket-content (`title: [My Title]`) flattens to its plain text,
            // dropping markup markers so a spine/feed title is clean text.
            ast::Expr::ContentBlock(c) => {
                Some(MetaValue::Str(markup_plain_text(c.body().to_untyped()).0))
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
            MetaValue::Array(items) => {
                TypstLiteral::Array(items.iter().map(MetaValue::to_literal).collect())
            }
        }
    }
}

/// A document title whose bracket content lost information when flattened to the
/// plain string rheo uses in the spine — carrying both forms so the caller can
/// warn the author.
#[derive(Debug, Clone, PartialEq)]
pub struct LossyTitle {
    /// The original title content as written (markup intact), e.g. `_Italic_ Title`.
    pub raw: String,
    /// The plain-text form kept in the spine, e.g. `Italic Title`.
    pub stripped: String,
}

/// All named arguments of the first `#set document(...)` rule in a vertebra,
/// captured generically as `(name, value)` pairs in source order.
///
/// Built from the first such rule in the tree (see [`SyntaxSite::first`]). Only
/// literal-valued arguments are retained (see [`MetaValue`]); an argument whose
/// value cannot be represented as a literal is silently skipped rather than
/// erroring, so an ordinary `date: datetime(...)` argument does not abort the
/// harvest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentMetadata {
    /// The harvested `(name, value)` pairs, in source order.
    pub fields: Vec<(String, MetaValue)>,
    /// Set when the `title` was bracket content that lost information (styling or
    /// sophisticated content) in the flattening to plain text.
    pub lossy_title: Option<LossyTitle>,
}

impl SyntaxSite for DocumentMetadata {
    const MAX_SITES: Option<usize> = Some(1);

    /// Match a top-level `set` rule targeting `document` and capture each of its
    /// named arguments whose value is a representable literal.
    ///
    /// Gated on `ctx.file_scope` so a `set document(...)` buried in a function
    /// body or closure — e.g. a `#let template(doc) = { set document(...); .. }`
    /// helper that this vertebra defines but never invokes — is not harvested:
    /// that rule only applies where the function is *called*, not here.
    fn visit(
        _source: &Source,
        node: &SyntaxNode,
        _offset: usize,
        ctx: WalkCtx,
        out: &mut Vec<Self>,
    ) {
        if ctx.file_scope
            && let Some(set_rule) = node.cast::<ast::SetRule>()
            && let ast::Expr::Ident(target) = set_rule.target()
            && target.as_str() == "document"
        {
            let mut fields = Vec::new();
            let mut lossy_title = None;
            for item in set_rule.args().items() {
                let ast::Arg::Named(named) = item else {
                    continue;
                };
                let name = named.name().as_str().to_string();
                let expr = named.expr();

                // A bracket-content title flattens to plain text; if that drops
                // any styling or sophisticated content, keep both forms so the
                // caller can warn the author.
                if name == "title"
                    && let ast::Expr::ContentBlock(c) = expr
                {
                    let body = c.body().to_untyped();
                    let (stripped, lossy) = markup_plain_text(body);
                    if lossy {
                        lossy_title = Some(LossyTitle {
                            raw: body.full_text().trim().to_string(),
                            stripped,
                        });
                    }
                }

                if let Some(v) = MetaValue::from_expr(expr) {
                    fields.push((name, v));
                }
            }
            out.push(DocumentMetadata {
                fields,
                lossy_title,
            });
        }
    }
}

impl DocumentMetadata {
    /// The value of a named argument (e.g. `title`), if present.
    pub fn get(&self, name: &str) -> Option<&MetaValue> {
        self.fields.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    /// Serialize the captured metadata to a [`TypstLiteral`] dictionary for the
    /// spine's `metadata` field. Empty metadata serializes to `(:)`.
    pub fn to_literal(&self) -> TypstLiteral {
        TypstLiteral::Dict(
            self.fields
                .iter()
                .map(|(k, v)| (k.clone(), v.to_literal()))
                .collect(),
        )
    }
}

/// Flatten a markup subtree to its plain text and report whether anything was
/// lost: concatenate every textual leaf, dropping markup markers (emphasis
/// underscores, `#strong[...]`, brackets) so `[Good news - #emph[Severance]]`
/// becomes `Good news - Severance`. Smart quotes are kept as their source
/// character, so `[She said "hi"]` keeps its quotes.
///
/// The returned bool is `true` when the content held anything beyond plain text
/// — styling (`_x_`, `*x*`) or sophisticated content (`$math$`, images, raw) —
/// i.e. the plain string is a lossy rendering of what the author wrote.
fn markup_plain_text(node: &SyntaxNode) -> (String, bool) {
    let mut out = String::new();
    let mut lossy = false;
    collect_text(node, &mut out, &mut lossy);
    (out.trim().to_string(), lossy)
}

/// Append the text of every textual leaf (`Text`/`Space`/`SmartQuote`) under
/// `node`, in order, recursing through wrapper nodes. `Markup`/`ContentBlock`
/// are transparent structure; encountering any *other* non-textual node (an
/// emphasis/strong wrapper, an equation, raw, an element call, …) means the
/// plain-text form drops something, so `lossy` is set.
fn collect_text(node: &SyntaxNode, out: &mut String, lossy: &mut bool) {
    match node.kind() {
        SyntaxKind::Text | SyntaxKind::Space | SyntaxKind::SmartQuote => {
            out.push_str(node.leaf_text())
        }
        SyntaxKind::Markup | SyntaxKind::ContentBlock => {
            for child in node.children() {
                collect_text(child, out, lossy);
            }
        }
        _ => {
            *lossy = true;
            for child in node.children() {
                collect_text(child, out, lossy);
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
    fn test_non_literal_scalar_args_skipped() {
        // Typst's document element rejects such args at compile time; even if
        // present in source, non-string scalars are not harvested.
        let m = metadata(r#"#set document(title: [T], count: 5, flag: true)"#);
        assert!(m.get("count").is_none());
        assert!(m.get("flag").is_none());
        assert_eq!(m.get("title").and_then(MetaValue::as_str), Some("T"));
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
        assert!(m.fields.is_empty());
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

    #[test]
    fn test_set_rule_inside_function_body_not_harvested() {
        // A `set document(...)` inside a `#let` helper only applies where the
        // helper is invoked, not in the file that merely defines it.
        let m = metadata(
            r#"#let template(doc) = {
  set document(title: "From Template")
  doc
}
= Heading"#,
        );
        assert!(m.get("title").is_none());
        assert!(m.fields.is_empty());
    }

    #[test]
    fn test_top_level_set_rule_still_harvested() {
        let m = metadata(r#"#set document(title: "Top Level")"#);
        assert_eq!(
            m.get("title").and_then(MetaValue::as_str),
            Some("Top Level")
        );
    }

    #[test]
    fn test_smart_quotes_survive_bracket_title() {
        let m = metadata(r#"#set document(title: [She said "hello"])"#);
        assert_eq!(
            m.get("title").and_then(MetaValue::as_str),
            Some("She said \"hello\"")
        );
    }

    #[test]
    fn test_lossy_title_flags_styling() {
        let m = metadata(r#"#set document(title: [_Italic_ Title])"#);
        assert_eq!(
            m.lossy_title,
            Some(LossyTitle {
                raw: "_Italic_ Title".to_string(),
                stripped: "Italic Title".to_string(),
            })
        );
    }

    #[test]
    fn test_lossy_title_flags_sophisticated_content() {
        let m = metadata(r#"#set document(title: [Chapter $x^2$])"#);
        let lossy = m.lossy_title.expect("math title should be lossy");
        assert_eq!(lossy.stripped, "Chapter");
    }

    #[test]
    fn test_plain_bracket_title_not_lossy() {
        let m = metadata(r#"#set document(title: [Hello World])"#);
        assert!(m.lossy_title.is_none());
    }

    #[test]
    fn test_string_title_not_lossy() {
        let m = metadata(r#"#set document(title: "Plain String")"#);
        assert!(m.lossy_title.is_none());
    }

    #[test]
    fn test_smart_quote_title_not_lossy() {
        // Quotes are preserved, so nothing is lost.
        let m = metadata(r#"#set document(title: [She said "hi"])"#);
        assert!(m.lossy_title.is_none());
    }
}

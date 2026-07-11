//! Extractor: `#set document(date: datetime(...))`.

use super::{SyntaxSite, WalkCtx};
use typst::syntax::{Source, SyntaxNode, ast};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn document_date(src: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        DocumentDate::first(&Source::detached(src)).map(|d| d.0)
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
}

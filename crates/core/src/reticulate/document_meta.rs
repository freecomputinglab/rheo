//! Rust-facing view of a compiled vertebra's Typst-resolved document metadata.
//!
//! [`DocumentMeta`] wraps typst's own `DocumentInfo` — the metadata Typst
//! resolved during realization of a `BundleDocument` (see
//! [`Build::compile_spine`](crate::build::Build)) — so Rust-side plugins
//! (EPUB) can read a vertebra's title/author/description/keywords/date from
//! the same fully-resolved values the Typst-side `metadata-of` beacon
//! publishes.

use ecow::EcoString;
use typst::foundations::{Datetime, Smart};
use typst::model::DocumentInfo;

/// Filename-to-title helper.
///
/// The real document title is read from `#set document(title: …)` by Typst
/// itself, post-compile (see [`DocumentMeta`]); this type only carries the
/// filename-derived fallback used pre-compile, and when a vertebra's output
/// has no resolved title at all.
pub struct DocumentTitle;

impl DocumentTitle {
    /// Convert a filename stem to a title-cased, human-readable name:
    /// separators become spaces, each word is capitalized.
    pub fn to_readable_name(filename: &str) -> String {
        filename
            .replace(['-', '_'], " ")
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A single vertebra's resolved `DocumentInfo`.
pub struct DocumentMeta(DocumentInfo);

impl DocumentMeta {
    /// Wrap an already-resolved `DocumentInfo`, typically cloned off a
    /// compiled `BundleDocument` (`Document::info`).
    pub fn new(info: DocumentInfo) -> Self {
        Self(info)
    }

    /// The resolved document title.
    ///
    /// Already flattened to plain text by Typst (`content.plain_text()`) —
    /// distinct from the Typst-side `metadata-of` beacon, which intentionally
    /// keeps rich content.
    pub fn title(&self) -> Option<&str> {
        self.0.title.as_deref()
    }

    /// The resolved document description.
    pub fn description(&self) -> Option<&str> {
        self.0.description.as_deref()
    }

    /// The resolved document author(s); empty when none were set.
    pub fn author(&self) -> &[EcoString] {
        &self.0.author
    }

    /// The resolved document keywords; empty when none were set.
    pub fn keywords(&self) -> &[EcoString] {
        &self.0.keywords
    }

    /// The resolved document date as a UTC `chrono` datetime.
    ///
    /// `Smart::Auto` (no `#set document(date:)` rule at all) and
    /// `Smart::Custom(None)` (an explicit `date: none`) both map to `None`.
    ///
    /// `#set document(date: datetime.today())` resolves to a real,
    /// build-varying date, indistinguishable here from a literal
    /// `datetime(...)` — anything syndicating this date downstream sees it
    /// change on every build.
    pub fn date(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        let Smart::Custom(Some(dt)) = self.0.date else {
            return None;
        };
        Self::to_chrono(dt)
    }

    /// Convert a typst `Datetime` into a UTC `chrono` datetime.
    ///
    /// Absent time components (a `Datetime::Date` has no time of day) default
    /// to midnight. Returns `None` when `year`/`month`/`day` aren't all
    /// present (e.g. a bare `Datetime::Time`) or don't form a valid calendar
    /// date.
    fn to_chrono(dt: Datetime) -> Option<chrono::DateTime<chrono::Utc>> {
        use chrono::TimeZone;

        let year = dt.year()?;
        let month = dt.month()?;
        let day = dt.day()?;
        let hour = dt.hour().unwrap_or(0);
        let minute = dt.minute().unwrap_or(0);
        let second = dt.second().unwrap_or(0);

        chrono::Utc
            .with_ymd_and_hms(
                year,
                month as u32,
                day as u32,
                hour as u32,
                minute as u32,
                second as u32,
            )
            .single()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filename_to_title() {
        assert_eq!(
            DocumentTitle::to_readable_name("severance-ep-1"),
            "Severance Ep 1"
        );
        assert_eq!(
            DocumentTitle::to_readable_name("my_document"),
            "My Document"
        );
        assert_eq!(DocumentTitle::to_readable_name("chapter-01"), "Chapter 01");
        assert_eq!(
            DocumentTitle::to_readable_name("hello_world"),
            "Hello World"
        );
        assert_eq!(DocumentTitle::to_readable_name("single"), "Single");
    }

    fn info_with_date(date: Smart<Option<Datetime>>) -> DocumentInfo {
        DocumentInfo {
            date,
            ..Default::default()
        }
    }

    #[test]
    fn date_is_none_for_auto() {
        let meta = DocumentMeta::new(info_with_date(Smart::Auto));
        assert_eq!(meta.date(), None);
    }

    #[test]
    fn date_is_none_for_explicit_none() {
        let meta = DocumentMeta::new(info_with_date(Smart::Custom(None)));
        assert_eq!(meta.date(), None);
    }

    #[test]
    fn date_resolves_a_real_datetime() {
        use chrono::TimeZone;
        let dt = Datetime::from_ymd_hms(2025, 1, 15, 10, 30, 0).unwrap();
        let meta = DocumentMeta::new(info_with_date(Smart::Custom(Some(dt))));
        assert_eq!(
            meta.date(),
            Some(
                chrono::Utc
                    .with_ymd_and_hms(2025, 1, 15, 10, 30, 0)
                    .unwrap()
            )
        );
    }

    #[test]
    fn date_resolves_a_date_only_datetime_at_midnight() {
        use chrono::TimeZone;
        let dt = Datetime::from_ymd(2025, 1, 15).unwrap();
        let meta = DocumentMeta::new(info_with_date(Smart::Custom(Some(dt))));
        assert_eq!(
            meta.date(),
            Some(chrono::Utc.with_ymd_and_hms(2025, 1, 15, 0, 0, 0).unwrap())
        );
    }

    #[test]
    fn title_author_description_keywords_read_through() {
        let info = DocumentInfo {
            title: Some("My Title".into()),
            author: vec!["Ada Lovelace".into()],
            description: Some("A description".into()),
            keywords: vec!["foo".into(), "bar".into()],
            ..Default::default()
        };
        let meta = DocumentMeta::new(info);
        assert_eq!(meta.title(), Some("My Title"));
        assert_eq!(meta.description(), Some("A description"));
        assert_eq!(meta.author(), [EcoString::from("Ada Lovelace")]);
        assert_eq!(meta.keywords().len(), 2);
    }
}

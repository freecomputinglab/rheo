//! Post-compile transclusion of compiled page HTML into Typst-authored assets.
//!
//! A `.marrow.typ` (a project's own, or one shipped by a package) can mint an
//! output file with `#asset("feed.xml", ...)`, but marrow runs *inside* the
//! Typst compile, before any page HTML exists — Typst has no
//! content-to-HTML-string function at all. So a Typst-authored artifact (an
//! Atom feed's `<content type="html">`, a sitemap, a search index, `llms.txt`)
//! cannot embed the compiled output of another page directly.
//!
//! This module closes that gap with a placeholder element resolved after
//! compilation, when compiled page HTML exists:
//!
//! ```text
//! <rheo-content page="notes/etal.html" select="main" as="escaped"/>
//! ```
//!
//! - `page` (required) — a compiled page's plugin-output-relative path.
//! - `select` (optional) — a bare tag name (`main`) or a leading-dot class
//!   (`.rheo-feed-content`); absent uses the default cascade (see
//!   [`crate::util::html::HtmlDom::select_inner_html`]).
//! - `as` (optional) — `escaped` (default; `&`/`<`/`>` entity-escaped, for
//!   Atom `<content type="html">`) or `raw` (verbatim, for
//!   `<content type="xhtml">`).
//!
//! Each placeholder is replaced by the *inner* HTML of the selected region.
//! Relative hrefs inside the transcluded HTML are not rewritten — a nested
//! page's `../foo.html` links are wrong in an absolute-URL context, but that
//! parity with the Rust feed generator being replaced is deliberate; an
//! absolutising `base` attribute is a separate, later concern.

use crate::plugins::CastVertebra;
use crate::util::html::{HtmlDom, escape_text};
use crate::{Result, RheoError};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use typst::foundations::Bytes;

lazy_static! {
    /// Matches a whole `<rheo-content .../>` self-closing tag, capturing its
    /// attribute text.
    static ref TAG_PATTERN: Regex =
        Regex::new(r#"<rheo-content\b([^>]*)/>"#).expect("invalid rheo-content tag pattern");

    /// Matches a single `name="value"` attribute pair within a tag's attribute
    /// text.
    static ref ATTR_PATTERN: Regex = Regex::new(r#"([a-zA-Z][a-zA-Z0-9_-]*)\s*=\s*"([^"]*)""#)
        .expect("invalid rheo-content attribute pattern");
}

/// How a transcluded region's HTML is inserted into the asset text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// `&`, `<`, `>` entity-escaped — what Atom `<content type="html">` needs.
    Escaped,
    /// Inserted verbatim — for Atom `<content type="xhtml">`.
    Raw,
}

/// A single `<rheo-content page="..." select="..." as="..."/>` placeholder
/// found in a Typst-authored asset, naming the compiled page (and region
/// within it) whose inner HTML replaces the placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentTransclusion {
    page: String,
    select: Option<String>,
    encoding: Encoding,
}

impl ContentTransclusion {
    /// Scan `text` for `<rheo-content .../>` placeholders, returning each
    /// match's byte range (for substitution) alongside its parsed attributes.
    ///
    /// A malformed placeholder missing the required `page` attribute is
    /// skipped (left as literal text) rather than partially parsed.
    pub fn scan(text: &str) -> Vec<(Range<usize>, Self)> {
        TAG_PATTERN
            .captures_iter(text)
            .filter_map(|caps| {
                let whole = caps.get(0).expect("group 0 always matches");
                let attrs_str = caps.get(1).map(|m| m.as_str()).unwrap_or_default();

                let mut page = None;
                let mut select = None;
                let mut encoding = Encoding::Escaped;
                for attr in ATTR_PATTERN.captures_iter(attrs_str) {
                    match &attr[1] {
                        "page" => page = Some(attr[2].to_string()),
                        "select" => select = Some(attr[2].to_string()),
                        "as" if &attr[2] == "raw" => encoding = Encoding::Raw,
                        _ => {}
                    }
                }

                let page = page?;
                Some((
                    whole.range(),
                    Self {
                        page,
                        select,
                        encoding,
                    },
                ))
            })
            .collect()
    }

    /// Resolve this placeholder against the main compile path's already-cast
    /// per-vertebra outputs.
    pub fn resolve(&self, outputs: &[CastVertebra]) -> Result<String> {
        let html = outputs
            .iter()
            .find(|v| v.output_path == self.page)
            .map(|v| String::from_utf8_lossy(v.bytes.as_slice()).into_owned());
        let available: Vec<&str> = outputs.iter().map(|v| v.output_path.as_str()).collect();
        self.resolve_html(html.as_deref(), &available)
    }

    /// Resolve this placeholder against a plain page-path → HTML map — the
    /// shape available to the dev-server preview path, which walks a raw
    /// `virtual_fs` rather than a `CastVertebra` list.
    pub fn resolve_from_map(&self, pages: &HashMap<String, String>) -> Result<String> {
        let html = pages.get(&self.page).map(|s| s.as_str());
        let available: Vec<&str> = pages.keys().map(|s| s.as_str()).collect();
        self.resolve_html(html, &available)
    }

    /// Shared resolution logic once the candidate page's HTML text (if any)
    /// has been looked up: missing-page diagnostic, region selection, and
    /// encoding.
    fn resolve_html(&self, html: Option<&str>, available: &[&str]) -> Result<String> {
        let html = html.ok_or_else(|| {
            let sample: Vec<&str> = available.iter().take(5).copied().collect();
            RheoError::invalid_data(format!(
                "<rheo-content> references unknown page '{}'; available output paths include: {}",
                self.page,
                sample.join(", ")
            ))
        })?;

        let dom = HtmlDom::parse(html)?;
        let inner = dom.select_inner_html(self.select.as_deref())?;

        Ok(match self.encoding {
            Encoding::Escaped => escape_text(&inner),
            Encoding::Raw => inner,
        })
    }

    /// Replace every placeholder in `text` (the asset named `asset_name`,
    /// folded into any error for diagnostics), resolving each match with
    /// `resolve_one`. Returns `None` — a signal to leave the original bytes
    /// untouched — when `text` contains no placeholder.
    fn rewrite_text(
        asset_name: &str,
        text: &str,
        resolve_one: impl Fn(&Self) -> Result<String>,
    ) -> Result<Option<String>> {
        let placeholders = Self::scan(text);
        if placeholders.is_empty() {
            return Ok(None);
        }

        let mut result = String::with_capacity(text.len());
        let mut last = 0;
        for (range, transclusion) in &placeholders {
            result.push_str(&text[last..range.start]);
            let resolved = resolve_one(transclusion)
                .map_err(|e| RheoError::invalid_data(format!("asset '{asset_name}': {e}")))?;
            result.push_str(&resolved);
            last = range.end;
        }
        result.push_str(&text[last..]);
        Ok(Some(result))
    }

    /// Rewrite every bundle-emitted asset in `asset_files` in place, replacing
    /// each `<rheo-content>` placeholder with the compiled inner HTML of the
    /// page it names, looked up in `outputs` (the main compile path's
    /// already-cast per-vertebra outputs).
    ///
    /// An asset whose bytes are not valid UTF-8 (an image, a font) is left
    /// byte-identical — this is not an error. An asset with no placeholder is
    /// also left byte-identical.
    pub fn rewrite_assets(
        outputs: &[CastVertebra],
        asset_files: &mut [(String, Bytes)],
    ) -> Result<()> {
        for (name, bytes) in asset_files.iter_mut() {
            let Ok(text) = std::str::from_utf8(bytes.as_slice()) else {
                continue;
            };
            if let Some(rewritten) = Self::rewrite_text(name, text, |t| t.resolve(outputs))? {
                *bytes = Bytes::new(rewritten.into_bytes());
            }
        }
        Ok(())
    }

    /// Rewrite one asset's text against the dev-server preview path's
    /// page → HTML map. Returns `None` — leave untouched — when `text`
    /// contains no placeholder.
    pub fn rewrite_from_map(
        asset_name: &str,
        text: &str,
        pages: &HashMap<String, String>,
    ) -> Result<Option<String>> {
        Self::rewrite_text(asset_name, text, |t| t.resolve_from_map(pages))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::TypstFormat;

    fn vertebra(path: &str, html: &str) -> CastVertebra {
        CastVertebra {
            output_path: path.to_string(),
            bytes: Bytes::new(html.as_bytes().to_vec()),
            format: TypstFormat::Html,
            title: String::new(),
            date: None,
            vars: HashMap::new(),
            contributed: false,
        }
    }

    fn scan_one(text: &str) -> ContentTransclusion {
        let mut matches = ContentTransclusion::scan(text);
        assert_eq!(matches.len(), 1, "expected exactly one placeholder");
        matches.remove(0).1
    }

    #[test]
    fn test_default_select_yields_escaped_main_content() {
        let outputs = vec![vertebra(
            "notes/etal.html",
            "<html><body><main><p>Body</p></main></body></html>",
        )];
        let transclusion = scan_one(r#"<rheo-content page="notes/etal.html"/>"#);
        let resolved = transclusion.resolve(&outputs).unwrap();
        assert_eq!(resolved, "&lt;p&gt;Body&lt;/p&gt;");
    }

    #[test]
    fn test_select_by_class_overrides_main_default() {
        let outputs = vec![vertebra(
            "page.html",
            "<html><body><main><p>main</p></main><div class=\"rheo-feed-content\"><p>class</p></div></body></html>",
        )];
        let transclusion =
            scan_one(r#"<rheo-content page="page.html" select=".rheo-feed-content"/>"#);
        let resolved = transclusion.resolve(&outputs).unwrap();
        assert_eq!(resolved, "&lt;p&gt;class&lt;/p&gt;");
    }

    #[test]
    fn test_select_by_bare_tag() {
        let outputs = vec![vertebra(
            "page.html",
            "<html><body><article><p>x</p></article></body></html>",
        )];
        let transclusion = scan_one(r#"<rheo-content page="page.html" select="article"/>"#);
        let resolved = transclusion.resolve(&outputs).unwrap();
        assert_eq!(resolved, "&lt;p&gt;x&lt;/p&gt;");
    }

    #[test]
    fn test_as_raw_is_unescaped() {
        let outputs = vec![vertebra(
            "page.html",
            "<html><body><main><p>Body</p></main></body></html>",
        )];
        let transclusion = scan_one(r#"<rheo-content page="page.html" as="raw"/>"#);
        let resolved = transclusion.resolve(&outputs).unwrap();
        assert_eq!(resolved, "<p>Body</p>");
    }

    #[test]
    fn test_unknown_page_errors_with_page_path_and_available() {
        let outputs = vec![vertebra("known.html", "<html><body><p>x</p></body></html>")];
        let transclusion = scan_one(r#"<rheo-content page="missing.html"/>"#);
        let err = transclusion.resolve(&outputs).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("missing.html"), "message was: {msg}");
        assert!(msg.contains("known.html"), "message was: {msg}");
    }

    #[test]
    fn test_non_utf8_asset_is_untouched() {
        let outputs: Vec<CastVertebra> = vec![];
        let bytes = Bytes::new(vec![0xff, 0xfe, 0x00, 0x01]);
        let mut asset_files = vec![("binary.dat".to_string(), bytes.clone())];
        ContentTransclusion::rewrite_assets(&outputs, &mut asset_files).unwrap();
        assert_eq!(asset_files[0].1.as_slice(), bytes.as_slice());
    }

    #[test]
    fn test_asset_with_no_placeholder_is_byte_identical() {
        let outputs: Vec<CastVertebra> = vec![];
        let bytes = Bytes::new(b"<xml>no placeholder here</xml>".to_vec());
        let mut asset_files = vec![("feed.xml".to_string(), bytes.clone())];
        ContentTransclusion::rewrite_assets(&outputs, &mut asset_files).unwrap();
        assert_eq!(asset_files[0].1.as_slice(), bytes.as_slice());
    }

    #[test]
    fn test_rewrite_assets_replaces_placeholder_in_place() {
        let outputs = vec![vertebra(
            "page.html",
            "<html><body><main><p>Body</p></main></body></html>",
        )];
        let text = r#"<content>before<rheo-content page="page.html"/>after</content>"#;
        let mut asset_files = vec![("feed.xml".to_string(), Bytes::new(text.as_bytes().to_vec()))];
        ContentTransclusion::rewrite_assets(&outputs, &mut asset_files).unwrap();
        let result = std::str::from_utf8(asset_files[0].1.as_slice()).unwrap();
        assert_eq!(
            result,
            "<content>before&lt;p&gt;Body&lt;/p&gt;after</content>"
        );
    }

    #[test]
    fn test_rewrite_assets_error_names_the_asset() {
        let outputs: Vec<CastVertebra> = vec![];
        let text = r#"<rheo-content page="missing.html"/>"#;
        let mut asset_files = vec![("feed.xml".to_string(), Bytes::new(text.as_bytes().to_vec()))];
        let err = ContentTransclusion::rewrite_assets(&outputs, &mut asset_files).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("feed.xml"), "message was: {msg}");
        assert!(msg.contains("missing.html"), "message was: {msg}");
    }

    #[test]
    fn test_resolve_from_map_matches_resolve() {
        let mut pages = HashMap::new();
        pages.insert(
            "page.html".to_string(),
            "<html><body><main><p>Body</p></main></body></html>".to_string(),
        );
        let transclusion = scan_one(r#"<rheo-content page="page.html"/>"#);
        let resolved = transclusion.resolve_from_map(&pages).unwrap();
        assert_eq!(resolved, "&lt;p&gt;Body&lt;/p&gt;");
    }

    #[test]
    fn test_scan_ignores_placeholder_missing_required_page() {
        let matches = ContentTransclusion::scan(r#"<rheo-content select="main"/>"#);
        assert!(matches.is_empty());
    }
}

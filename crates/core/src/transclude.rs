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
//!   (`.rheo-content`); absent uses the default cascade (see
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
use crate::{CONTROL_ASSET_PREFIX, Result, RheoError};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use tracing::warn;
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

/// The outcome of classifying a single bundle-asset path against
/// [`CONTROL_ASSET_PREFIX`]: an ordinary asset that must pass through
/// untouched, the recognized site-wide head fragment (decoded), or an
/// unrecognized `.rheo/*` member (already warned about, must be dropped).
pub(crate) enum ControlAssetKind {
    /// Not a control asset — leave it in the caller's asset list.
    NotControl,
    /// `.rheo/head.html`, decoded to UTF-8 text.
    HeadFragment(String),
    /// An unrecognized `.rheo/*` path. Already logged via `warn!`; the caller
    /// must drop it rather than write/serve/embed it.
    UnrecognizedDropped,
}

/// Bundle-root control assets consumed internally by rheo rather than
/// forwarded to a format plugin.
///
/// A `.marrow.typ` runs at the bundle root, outside every page's own
/// `#document`, so it cannot place an element inside any single page's
/// `<head>` (that's what `<rheo-head>`/[`HtmlDom::hoist_rheo_head`] is for,
/// per-page). `ControlAssets::extract` pulls the one currently-recognized
/// member — `.rheo/head.html`, an HTML fragment whose top-level elements are
/// appended to *every* compiled page's `<head>` — out of a plugin's asset
/// list before that list reaches the plugin, so it is never written to disk,
/// embedded in a container format, or served verbatim.
///
/// A project with no `.rheo/*` assets at all makes `extract` a complete
/// no-op: `head_fragment: None`, and the returned asset list is unchanged in
/// content and order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlAssets {
    /// Decoded contents of `.rheo/head.html`, if present — an HTML fragment
    /// (no wrapping `<html>/<head>/<body>`) whose top-level elements are
    /// appended to every compiled page's `<head>`, via
    /// [`HtmlDom::append_head_fragment`].
    pub head_fragment: Option<String>,
}

impl ControlAssets {
    /// Classify one `(path, bytes)` bundle-asset entry.
    ///
    /// Shared by the main compile path (which owns and filters a
    /// `Vec<(String, Bytes)>` via [`Self::extract`]) and the dev-server watch
    /// path (which only needs to scan a borrowed `VirtualFs` for the same
    /// recognized/unrecognized distinction), so the UTF-8-decode-or-error and
    /// unrecognized-warn logic lives in exactly one place.
    pub(crate) fn classify_asset(path: &str, bytes: &Bytes) -> Result<ControlAssetKind> {
        let Some(rest) = path.strip_prefix(CONTROL_ASSET_PREFIX) else {
            return Ok(ControlAssetKind::NotControl);
        };

        if rest == "head.html" {
            let text = std::str::from_utf8(bytes.as_slice())
                .map_err(|e| {
                    RheoError::invalid_data(format!(
                        "control asset '{path}' is not valid UTF-8: {e}"
                    ))
                })?
                .to_string();
            Ok(ControlAssetKind::HeadFragment(text))
        } else {
            warn!(path, "unrecognized control asset under .rheo/, dropping");
            Ok(ControlAssetKind::UnrecognizedDropped)
        }
    }

    /// Returns `true` when `path` names a control asset — anything under
    /// [`CONTROL_ASSET_PREFIX`], recognized or not — that must never be
    /// written, embedded, or served.
    pub fn is_control_asset(path: &str) -> bool {
        path.starts_with(CONTROL_ASSET_PREFIX)
    }

    /// Partition `assets` into control assets (consumed here) and everything
    /// else (returned for the plugin to keep writing/embedding as before).
    ///
    /// Recognized control assets populate the returned `ControlAssets`;
    /// unrecognized `.rheo/*` paths are logged via `warn!` and dropped
    /// silently, so a newer package against an older rheo degrades
    /// gracefully rather than hard-failing the build.
    pub fn extract(assets: Vec<(String, Bytes)>) -> Result<(Vec<(String, Bytes)>, Self)> {
        let mut head_fragment = None;
        let mut remaining = Vec::with_capacity(assets.len());

        for (path, bytes) in assets {
            match Self::classify_asset(&path, &bytes)? {
                ControlAssetKind::NotControl => remaining.push((path, bytes)),
                ControlAssetKind::HeadFragment(text) => head_fragment = Some(text),
                ControlAssetKind::UnrecognizedDropped => {}
            }
        }

        Ok((remaining, Self { head_fragment }))
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
            description: None,
            keywords: vec![],
            author: vec![],
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
            "<html><body><main><p>main</p></main><div class=\"rheo-content\"><p>class</p></div></body></html>",
        )];
        let transclusion = scan_one(r#"<rheo-content page="page.html" select=".rheo-content"/>"#);
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

    // ControlAssets::extract tests

    fn asset(path: &str, bytes: &[u8]) -> (String, Bytes) {
        (path.to_string(), Bytes::new(bytes.to_vec()))
    }

    #[test]
    fn test_extract_recognizes_head_html_and_removes_it_from_remaining() {
        let assets = vec![
            asset(".rheo/head.html", b"<meta name=\"a\" content=\"1\">"),
            asset("extra/hello.txt", b"hi"),
        ];
        let (remaining, control) = ControlAssets::extract(assets).unwrap();

        assert_eq!(
            control.head_fragment.as_deref(),
            Some(r#"<meta name="a" content="1">"#)
        );
        assert_eq!(remaining.len(), 1, "the .rheo/head.html entry must be gone");
        assert_eq!(remaining[0].0, "extra/hello.txt");
    }

    #[test]
    fn test_extract_drops_unrecognized_control_asset() {
        let assets = vec![
            asset(".rheo/future-thing.json", b"{}"),
            asset("extra/hello.txt", b"hi"),
        ];
        let (remaining, control) = ControlAssets::extract(assets).unwrap();

        assert_eq!(control.head_fragment, None);
        assert_eq!(
            remaining.len(),
            1,
            "unrecognized control asset must be dropped, not written"
        );
        assert_eq!(remaining[0].0, "extra/hello.txt");
    }

    #[test]
    fn test_extract_no_rheo_assets_is_complete_noop() {
        let assets = vec![asset("extra/hello.txt", b"hi"), asset("a.css", b"body{}")];
        let (remaining, control) = ControlAssets::extract(assets.clone()).unwrap();

        assert_eq!(control.head_fragment, None);
        assert_eq!(
            remaining, assets,
            "content and order must be byte-identical with no .rheo/* assets present"
        );
    }

    #[test]
    fn test_extract_non_utf8_head_html_errors() {
        let assets = vec![asset(".rheo/head.html", &[0xff, 0xfe, 0xfd])];
        let err = ControlAssets::extract(assets).unwrap_err();
        assert!(err.to_string().contains(".rheo/head.html"));
    }

    #[test]
    fn test_is_control_asset() {
        assert!(ControlAssets::is_control_asset(".rheo/head.html"));
        assert!(ControlAssets::is_control_asset(".rheo/whatever"));
        assert!(!ControlAssets::is_control_asset("extra/hello.txt"));
    }
}

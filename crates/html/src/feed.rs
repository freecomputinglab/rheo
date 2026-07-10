//! Atom 1.0 feed model (RFC 4287).
//!
//! [`AtomFeed`]/[`AtomEntry`] are the rheo-side domain model; serialization to
//! XML is delegated to the `atom_syndication` crate via [`AtomFeed::serialize`].
//! The wiring that builds an [`AtomFeed`] from the compiled spine lives in the
//! plugin's `compile` (Issue E).

use atom_syndication as atom;
use chrono::{DateTime, Utc};
use rheo_core::util::html as html_utils;
use rheo_core::{CastVertebra, PluginContext, RheoError};

/// A single `<entry>` in the feed.
pub struct AtomEntry {
    pub id: String,
    pub title: String,
    pub updated: DateTime<Utc>,
    /// Page body HTML, emitted as `<content type="html">` (escaped by the serializer).
    pub content_html: String,
    /// Target of the `rel="alternate"` link (the page URL).
    pub alternate_href: String,
}

/// A complete Atom feed.
pub struct AtomFeed {
    pub id: String,
    pub title: String,
    pub updated: DateTime<Utc>,
    /// Target of the `rel="self"` link (the feed URL).
    pub self_href: String,
    /// Feed-level author name, emitted as `<author><name>...</name></author>`.
    pub author: String,
    pub entries: Vec<AtomEntry>,
}

impl AtomFeed {
    /// Render this feed to an RFC 4287 Atom XML string.
    pub fn serialize(&self) -> String {
        let mut author = atom::Person::default();
        author.set_name(self.author.clone());

        let entries = self
            .entries
            .iter()
            .map(AtomEntry::to_atom)
            .collect::<Vec<_>>();

        let mut feed = atom::Feed::default();
        feed.set_id(self.id.clone());
        feed.set_title(self.title.clone());
        feed.set_updated(self.updated.fixed_offset());
        feed.set_authors(vec![author]);
        feed.set_links(vec![link("self", &self.self_href)]);
        feed.set_entries(entries);
        feed.to_string()
    }
}

impl AtomEntry {
    fn to_atom(&self) -> atom::Entry {
        let mut content = atom::Content::default();
        content.set_value(self.content_html.clone());
        content.set_content_type("html".to_string());

        let mut entry = atom::Entry::default();
        entry.set_id(self.id.clone());
        entry.set_title(self.title.clone());
        entry.set_updated(self.updated.fixed_offset());
        entry.set_links(vec![link("alternate", &self.alternate_href)]);
        entry.set_content(content);
        entry
    }
}

/// Build an Atom `<link rel="..." href="..."/>`.
fn link(rel: &str, href: &str) -> atom::Link {
    let mut l = atom::Link::default();
    l.set_rel(rel);
    l.set_href(href.to_string());
    l
}

/// Generate Atom feed from spine outputs when feed_base_url is configured.
///
/// Every vertebra produces a feed entry by default. Each entry's title defaults
/// to the parsed `#document` title (overridable with `rheo-feed-title`), and its
/// timestamp defaults to the parsed `#document` date, then the output file mtime
/// (overridable with `rheo-feed-updated`).
pub fn generate_feed(
    ctx: PluginContext<'_>,
    outputs: &[CastVertebra],
    base_url: &str,
    feed_title: &str,
    html_cfg: &super::HtmlConfig,
) -> Result<(), RheoError> {
    let feed_author = html_cfg.feed_author.as_deref().unwrap_or("Rheo");

    let mut entries = Vec::new();
    let mut max_updated = DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);

    for output in outputs {
        // Opt-out: skip vertebrae that declare rheo-feed-exclude = true.
        if output.vars.get("feed-exclude").and_then(|v| v.as_bool()) == Some(true) {
            continue;
        }

        // Entry title: rheo-feed-title override, else the parsed #document title.
        let title = match output.vars.get("feed-title").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => output.title.clone(),
        };

        // Entry timestamp precedence: rheo-feed-updated override, else the
        // parsed #document date, else the output file mtime.
        let updated = if let Some(ts) = output.vars.get("feed-updated").and_then(|v| v.as_str()) {
            DateTime::parse_from_rfc3339(ts)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| RheoError::invalid_data(format!("invalid rheo-feed-updated: {}", e)))?
        } else if let Some(date) = output.date {
            date
        } else {
            // Fall back to output file mtime.
            let out_path = ctx.output_dir.join(&output.output_path);
            std::fs::metadata(&out_path)
                .map_err(|e| {
                    RheoError::io(e, format!("reading metadata for {}", out_path.display()))
                })?
                .modified()
                .map_err(|e| RheoError::io(e, format!("reading mtime for {}", out_path.display())))?
                .into()
        };

        // Track max updated timestamp for feed-level <updated>.
        if updated > max_updated {
            max_updated = updated;
        }

        // Extract feed content from compiled HTML.
        let html_string = String::from_utf8(output.bytes.to_vec()).map_err(|e| {
            RheoError::invalid_data(format!("HTML output is not valid UTF-8: {}", e))
        })?;
        let dom = html_utils::HtmlDom::parse(&html_string)?;
        let content_html = dom.feed_content_inner_html()?;

        // Build entry URL: base_url + "/" + output_path (e.g. "https://example.com/chapter1.html").
        let alternate_href = format!("{}/{}", base_url.trim_end_matches('/'), output.output_path);

        // Build entry ID: use the URL as the ID (simplest approach).
        let id = alternate_href.clone();

        entries.push(AtomEntry {
            id,
            title,
            updated,
            content_html,
            alternate_href,
        });
    }

    // Skip feed generation if no entries.
    let entry_count = entries.len();
    if entry_count == 0 {
        return Ok(());
    }

    let feed = AtomFeed {
        id: format!("{}/feed.xml", base_url.trim_end_matches('/')),
        title: feed_title.to_string(),
        updated: max_updated,
        self_href: format!("{}/feed.xml", base_url.trim_end_matches('/')),
        author: feed_author.to_string(),
        entries,
    };

    let feed_xml = feed.serialize();
    let feed_path = ctx.output_dir.join("feed.xml");
    std::fs::write(&feed_path, feed_xml)
        .map_err(|e| RheoError::io(e, format!("writing feed to {}", feed_path.display())))?;
    tracing::info!(feed = %feed_path.display(), entries = entry_count, "generated Atom feed");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2025-01-15T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn entry(id: &str, title: &str) -> AtomEntry {
        AtomEntry {
            id: id.to_string(),
            title: title.to_string(),
            updated: ts(),
            content_html: "<p>Body</p>".to_string(),
            alternate_href: format!("https://example.com/{id}.html"),
        }
    }

    #[test]
    fn test_serialize_has_namespace_and_feed_elements() {
        let feed = AtomFeed {
            id: "https://example.com/feed.xml".to_string(),
            title: "My Blog".to_string(),
            updated: ts(),
            self_href: "https://example.com/feed.xml".to_string(),
            author: "Ada Lovelace".to_string(),
            entries: vec![entry("post", "First Post")],
        };
        let xml = feed.serialize();

        assert!(xml.contains(r#"<feed xmlns="http://www.w3.org/2005/Atom">"#));
        assert!(xml.contains("<id>https://example.com/feed.xml</id>"));
        assert!(xml.contains("<title>My Blog</title>"));
        assert!(xml.contains("<name>Ada Lovelace</name>"));
        assert!(xml.contains(r#"rel="self""#));
        assert!(xml.contains(r#"href="https://example.com/feed.xml""#));
        // Entry
        assert!(xml.contains("<entry>"));
        assert!(xml.contains("<title>First Post</title>"));
        assert!(xml.contains(r#"rel="alternate""#));
        assert!(xml.contains(r#"href="https://example.com/post.html""#));
        // Content is type=html and HTML-escaped by the serializer
        assert!(xml.contains(r#"type="html""#));
        assert!(xml.contains("&lt;p&gt;Body&lt;/p&gt;"));
    }

    #[test]
    fn test_serialize_multiple_entries() {
        let feed = AtomFeed {
            id: "id".to_string(),
            title: "t".to_string(),
            updated: ts(),
            self_href: "self".to_string(),
            author: "Rheo".to_string(),
            entries: vec![entry("a", "A"), entry("b", "B")],
        };
        let xml = feed.serialize();
        assert_eq!(xml.matches("<entry>").count(), 2);
    }

    #[test]
    fn test_feed_exclude_omits_entry() {
        use rheo_core::config::project::{ProjectConfig, ProjectMode};
        use rheo_core::{
            PluginSection, RheoConfig, RheoValue, SpineOptions, SpineSites, TypstFormat,
        };
        use std::collections::HashMap;
        use typst::foundations::Bytes;

        let dir = tempfile::tempdir().expect("tempdir");
        let output_dir = dir.path().to_path_buf();

        // Build a CastVertebra; `exclude` sets rheo-feed-exclude = <bool>.
        // A parsed `date` is supplied so the mtime fallback never touches disk.
        let make = |path: &str, exclude: Option<bool>| {
            let mut vars = HashMap::new();
            if let Some(val) = exclude {
                vars.insert("feed-exclude".to_string(), RheoValue::Bool(val));
            }
            CastVertebra {
                output_path: path.to_string(),
                bytes: Bytes::new(b"<html><body><main><p>Body</p></main></body></html>".to_vec()),
                format: TypstFormat::Html,
                title: path.to_string(),
                date: Some(ts()),
                vars,
            }
        };

        // keep.html included, skip.html excluded (true), false.html included (false).
        let outputs = vec![
            make("keep.html", None),
            make("skip.html", Some(true)),
            make("false.html", Some(false)),
        ];

        let project = ProjectConfig {
            name: "test".to_string(),
            root: output_dir.clone(),
            config: RheoConfig::default(),
            typ_files: vec![],
            mode: ProjectMode::Directory,
            config_path: None,
        };
        let spine = SpineOptions {
            title: None,
            vertebrae: vec![],
        };
        let section = PluginSection::default();
        let assets = HashMap::new();
        let font_dirs: Vec<std::path::PathBuf> = vec![];
        let ctx = PluginContext {
            project: &project,
            output_dir: &output_dir,
            spine: &spine,
            config: &section,
            assets: &assets,
            font_dirs: &font_dirs,
            site_map: SpineSites::empty(),
        };
        let html_cfg = crate::HtmlConfig::default();

        generate_feed(ctx, &outputs, "https://example.com", "Test Feed", &html_cfg)
            .expect("generate_feed");

        let feed = std::fs::read_to_string(output_dir.join("feed.xml")).expect("read feed.xml");
        // Only the excluded vertebra is dropped; "false" is not the strict match.
        assert_eq!(
            feed.matches("<entry>").count(),
            2,
            "expected 2 entries (skip.html excluded), feed:\n{feed}"
        );
        assert!(feed.contains("https://example.com/keep.html"));
        assert!(feed.contains("https://example.com/false.html"));
        assert!(!feed.contains("skip.html"), "excluded vertebra leaked");
    }

    #[test]
    fn test_serialize_escapes_title() {
        let feed = AtomFeed {
            id: "id".to_string(),
            title: r#"Tom & Jerry <3"#.to_string(),
            updated: ts(),
            self_href: "self".to_string(),
            author: "Rheo".to_string(),
            entries: vec![],
        };
        let xml = feed.serialize();
        assert!(xml.contains("Tom &amp; Jerry &lt;3"));
    }
}

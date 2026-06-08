//! Atom 1.0 feed model (RFC 4287).
//!
//! [`AtomFeed`]/[`AtomEntry`] are the rheo-side domain model; serialization to
//! XML is delegated to the `atom_syndication` crate via [`AtomFeed::serialize`].
//! The wiring that builds an [`AtomFeed`] from the compiled spine lives in the
//! plugin's `compile` (Issue E).

use atom_syndication as atom;
use chrono::{DateTime, Utc};

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
    pub entries: Vec<AtomEntry>,
}

impl AtomFeed {
    /// Render this feed to an RFC 4287 Atom XML string.
    pub fn serialize(&self) -> String {
        let mut author = atom::Person::default();
        author.set_name("Rheo");

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
            entries: vec![entry("post", "First Post")],
        };
        let xml = feed.serialize();

        assert!(xml.contains(r#"<feed xmlns="http://www.w3.org/2005/Atom">"#));
        assert!(xml.contains("<id>https://example.com/feed.xml</id>"));
        assert!(xml.contains("<title>My Blog</title>"));
        assert!(xml.contains("<name>Rheo</name>"));
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
            entries: vec![entry("a", "A"), entry("b", "B")],
        };
        let xml = feed.serialize();
        assert_eq!(xml.matches("<entry>").count(), 2);
    }

    #[test]
    fn test_serialize_escapes_title() {
        let feed = AtomFeed {
            id: "id".to_string(),
            title: r#"Tom & Jerry <3"#.to_string(),
            updated: ts(),
            self_href: "self".to_string(),
            entries: vec![],
        };
        let xml = feed.serialize();
        assert!(xml.contains("Tom &amp; Jerry &lt;3"));
    }
}

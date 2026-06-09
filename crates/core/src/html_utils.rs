//! HTML utilities for parsing, manipulating, and serializing HTML documents.
//!
//! Provides DOM manipulation via html5ever and head-injection helpers used by
//! the html plugin and any other crate that needs to post-process HTML output.

use crate::{Result, RheoError};
use html5ever::{ParseOpts, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::fmt::Write as _;

// ─── DOM types ───────────────────────────────────────────────────────────────

/// Wrapper around html5ever's RcDom for type-safe DOM manipulation.
pub struct HtmlDom {
    dom: RcDom,
}

impl HtmlDom {
    /// Parse an HTML string into a DOM tree.
    pub fn parse(html: &str) -> Result<Self> {
        let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default())
            .from_utf8()
            .read_from(&mut html.as_bytes())
            .map_err(|e| RheoError::HtmlGeneration {
                count: 1,
                errors: format!("failed to parse HTML: {}", e),
            })?;
        Ok(Self { dom })
    }

    /// Serialize the DOM tree back to an HTML string.
    pub fn serialize(&self) -> Result<String> {
        let mut output = String::new();
        serialize_node(&self.dom.document, &mut output)?;
        Ok(output)
    }

    /// Find an element by tag name (depth-first search).
    pub fn find_element(&self, tag_name: &str) -> Option<Element> {
        find_element_by_tag(&self.dom.document, tag_name).map(|handle| Element { handle })
    }

    /// Inject an Atom autodiscovery `<link>` into the HTML `<head>`.
    ///
    /// Inserts `<link rel="alternate" type="application/atom+xml" title="..." href="..."/>`
    /// after the last `<meta>` tag (or at position 0 if none).
    pub fn inject_feed_link(&mut self, href: &str, title: &str) -> Result<()> {
        let head = self
            .find_element("head")
            .ok_or_else(|| RheoError::HtmlGeneration {
                count: 1,
                errors: "HTML document does not contain a <head> element".to_string(),
            })?;

        let insert_pos = head.last_meta_index().map(|i| i + 1).unwrap_or(0);
        head.insert_child_at(insert_pos, Element::create_feed_link(href, title));
        Ok(())
    }

    /// Inject `<link>` and `<script>` elements into the HTML `<head>`.
    ///
    /// Nodes are inserted after the last `<meta>` tag (or at position 0 if none),
    /// in order: fonts, stylesheets, scripts.
    pub fn inject_head_links(
        &mut self,
        fonts: &[&str],
        stylesheets: &[&str],
        scripts: &[&str],
    ) -> Result<()> {
        let head = self
            .find_element("head")
            .ok_or_else(|| RheoError::HtmlGeneration {
                count: 1,
                errors: "HTML document does not contain a <head> element".to_string(),
            })?;

        let insert_pos = head.last_meta_index().map(|i| i + 1).unwrap_or(0);

        let mut offset = 0;
        for font in fonts {
            head.insert_child_at(
                insert_pos + offset,
                Element::create_link("stylesheet", font),
            );
            offset += 1;
        }
        for stylesheet in stylesheets {
            head.insert_child_at(
                insert_pos + offset,
                Element::create_link("stylesheet", stylesheet),
            );
            offset += 1;
        }
        for script in scripts {
            head.insert_child_at(insert_pos + offset, Element::create_script(script));
            offset += 1;
        }

        Ok(())
    }

    /// Serialize the inner HTML of the `<body>` element: its children without
    /// the surrounding `<body>` tag.
    ///
    /// Returns an error if the document has no `<body>` element. Head mutations
    /// (e.g. `inject_head_links`) do not affect this output, so callers may read
    /// the body before or after injecting head links.
    pub fn body_inner_html(&self) -> Result<String> {
        let body = find_element_by_tag(&self.dom.document, "body").ok_or_else(|| {
            RheoError::HtmlGeneration {
                count: 1,
                errors: "HTML document does not contain a <body> element".to_string(),
            }
        })?;
        inner_html(&body)
    }

    /// Serialize the inner HTML of the feed content region for one page.
    ///
    /// Resolution order, first match wins:
    /// 1. the first `<main>` element's inner HTML;
    /// 2. the first element carrying the `rheo-feed-content` class;
    /// 3. the whole `<body>` inner HTML.
    ///
    /// This lets authors scope feed entries to the article, excluding page
    /// chrome (header/footer/nav), by wrapping the article in `<main>` (or an
    /// element with class `rheo-feed-content`) and keeping the chrome outside
    /// it. With no marker present it falls back to the full body.
    pub fn feed_content_inner_html(&self) -> Result<String> {
        if let Some(main) = find_element_by_tag(&self.dom.document, "main") {
            return inner_html(&main);
        }
        if let Some(el) = find_element_by_class(&self.dom.document, "rheo-feed-content") {
            return inner_html(&el);
        }
        self.body_inner_html()
    }

    #[cfg(test)]
    pub fn document_root(&self) -> &Handle {
        &self.dom.document
    }
}

/// Wrapper around html5ever's Handle for type-safe element manipulation.
pub struct Element {
    handle: Handle,
}

impl Element {
    /// Create an Atom autodiscovery `<link>` element:
    /// `<link rel="alternate" type="application/atom+xml" title="..." href="...">`.
    pub fn create_feed_link(href: &str, title: &str) -> Self {
        use html5ever::tendril::StrTendril;
        use html5ever::{Attribute, LocalName, QualName, ns};
        use markup5ever_rcdom::Node;
        use std::cell::RefCell;

        let attrs = vec![
            Attribute {
                name: QualName::new(None, ns!(), LocalName::from("rel")),
                value: StrTendril::from("alternate"),
            },
            Attribute {
                name: QualName::new(None, ns!(), LocalName::from("type")),
                value: StrTendril::from("application/atom+xml"),
            },
            Attribute {
                name: QualName::new(None, ns!(), LocalName::from("title")),
                value: StrTendril::from(title),
            },
            Attribute {
                name: QualName::new(None, ns!(), LocalName::from("href")),
                value: StrTendril::from(href),
            },
        ];

        let handle = Node::new(NodeData::Element {
            name: QualName::new(None, ns!(html), LocalName::from("link")),
            attrs: RefCell::new(attrs),
            template_contents: RefCell::new(None),
            mathml_annotation_xml_integration_point: false,
        });

        Self { handle }
    }

    /// Create a `<link rel="..." href="...">` element.
    pub fn create_link(rel: &str, href: &str) -> Self {
        use html5ever::tendril::StrTendril;
        use html5ever::{Attribute, LocalName, QualName, ns};
        use markup5ever_rcdom::Node;
        use std::cell::RefCell;

        let attrs = vec![
            Attribute {
                name: QualName::new(None, ns!(), LocalName::from("rel")),
                value: StrTendril::from(rel),
            },
            Attribute {
                name: QualName::new(None, ns!(), LocalName::from("href")),
                value: StrTendril::from(href),
            },
        ];

        let handle = Node::new(NodeData::Element {
            name: QualName::new(None, ns!(html), LocalName::from("link")),
            attrs: RefCell::new(attrs),
            template_contents: RefCell::new(None),
            mathml_annotation_xml_integration_point: false,
        });

        Self { handle }
    }

    /// Create a `<script src="..."></script>` element.
    pub fn create_script(src: &str) -> Self {
        use html5ever::tendril::StrTendril;
        use html5ever::{Attribute, LocalName, QualName, ns};
        use markup5ever_rcdom::Node;
        use std::cell::RefCell;

        let attrs = vec![
            Attribute {
                name: QualName::new(None, ns!(), LocalName::from("src")),
                value: StrTendril::from(src),
            },
            Attribute {
                name: QualName::new(None, ns!(), LocalName::from("defer")),
                value: StrTendril::from(""),
            },
        ];

        let handle = Node::new(NodeData::Element {
            name: QualName::new(None, ns!(html), LocalName::from("script")),
            attrs: RefCell::new(attrs),
            template_contents: RefCell::new(None),
            mathml_annotation_xml_integration_point: false,
        });

        Self { handle }
    }

    /// Prepend a child element to this element.
    pub fn prepend_child(&self, child: Element) {
        let mut children = self.handle.children.borrow_mut();
        children.insert(0, child.handle);
    }

    /// Insert a child element at the given index (clamped to children length).
    pub fn insert_child_at(&self, index: usize, child: Element) {
        let mut children = self.handle.children.borrow_mut();
        let index = index.min(children.len());
        children.insert(index, child.handle);
    }

    /// Returns the index of the last `<meta>` child in this element's children, if any.
    fn last_meta_index(&self) -> Option<usize> {
        let children = self.handle.children.borrow();
        children
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, child)| match &child.data {
                NodeData::Element { name, .. } if name.local.as_ref() == "meta" => Some(i),
                _ => None,
            })
    }

    #[cfg(test)]
    pub fn tag_name(&self) -> &str {
        match &self.handle.data {
            NodeData::Element { name, .. } => name.local.as_ref(),
            _ => "",
        }
    }
}

fn find_element_by_tag(handle: &Handle, tag_name: &str) -> Option<Handle> {
    match &handle.data {
        NodeData::Element { name, .. } if name.local.as_ref() == tag_name => {
            return Some(handle.clone());
        }
        _ => {}
    }

    for child in handle.children.borrow().iter() {
        if let Some(found) = find_element_by_tag(child, tag_name) {
            return Some(found);
        }
    }

    None
}

/// Find the first element (depth-first) whose `class` attribute contains
/// `class` as a whitespace-separated token (not a substring match).
fn find_element_by_class(handle: &Handle, class: &str) -> Option<Handle> {
    if let NodeData::Element { attrs, .. } = &handle.data {
        let matches = attrs.borrow().iter().any(|attr| {
            attr.name.local.as_ref() == "class" && attr.value.split_whitespace().any(|c| c == class)
        });
        if matches {
            return Some(handle.clone());
        }
    }

    for child in handle.children.borrow().iter() {
        if let Some(found) = find_element_by_class(child, class) {
            return Some(found);
        }
    }

    None
}

/// Serialize the inner HTML of an element: its children without the surrounding
/// tag.
fn inner_html(handle: &Handle) -> Result<String> {
    let mut output = String::new();
    for child in handle.children.borrow().iter() {
        serialize_node(child, &mut output)?;
    }
    Ok(output)
}

fn serialize_node(handle: &Handle, output: &mut String) -> Result<()> {
    match &handle.data {
        NodeData::Document => {
            for child in handle.children.borrow().iter() {
                serialize_node(child, output)?;
            }
        }
        NodeData::Doctype { name, .. } => {
            write!(output, "<!DOCTYPE {}>", name).map_err(|e| RheoError::HtmlGeneration {
                count: 1,
                errors: format!("failed to serialize doctype: {}", e),
            })?;
        }
        NodeData::Text { contents } => {
            let text = contents.borrow();
            let escaped = text
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            output.push_str(&escaped);
        }
        NodeData::Comment { contents } => {
            write!(output, "<!--{}-->", contents).map_err(|e| RheoError::HtmlGeneration {
                count: 1,
                errors: format!("failed to serialize comment: {}", e),
            })?;
        }
        NodeData::Element { name, attrs, .. } => {
            write!(output, "<{}", name.local).map_err(|e| RheoError::HtmlGeneration {
                count: 1,
                errors: format!("failed to serialize element: {}", e),
            })?;

            for attr in attrs.borrow().iter() {
                write!(output, " {}=\"{}\"", attr.name.local, attr.value).map_err(|e| {
                    RheoError::HtmlGeneration {
                        count: 1,
                        errors: format!("failed to serialize attribute: {}", e),
                    }
                })?;
            }

            if is_void_element(&name.local) {
                output.push('>');
            } else {
                output.push('>');
                for child in handle.children.borrow().iter() {
                    serialize_node(child, output)?;
                }
                write!(output, "</{}>", name.local).map_err(|e| RheoError::HtmlGeneration {
                    count: 1,
                    errors: format!("failed to serialize closing tag: {}", e),
                })?;
            }
        }
        NodeData::ProcessingInstruction { target, contents } => {
            write!(output, "<?{} {}?>", target, contents).map_err(|e| {
                RheoError::HtmlGeneration {
                    count: 1,
                    errors: format!("failed to serialize processing instruction: {}", e),
                }
            })?;
        }
    }
    Ok(())
}

fn is_void_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

// ─── Head injection utilities ─────────────────────────────────────────────────

/// Embed CSS content directly into the HTML `<head>` as `<style>` blocks.
///
/// Uses string manipulation rather than DOM parsing to avoid escaping issues
/// with CSS selectors (e.g. `p > span`).
///
/// Returns an error if the HTML does not contain a `</head>` tag.
pub fn inject_inline_styles(html: &str, css_blocks: &[&str]) -> Result<String> {
    if css_blocks.is_empty() {
        return Ok(html.to_string());
    }

    let mut styles = String::new();
    for css in css_blocks {
        styles.push_str("<style>");
        styles.push_str(css);
        styles.push_str("</style>");
    }

    if let Some(pos) = html.find("</head>") {
        let mut result = String::with_capacity(html.len() + styles.len());
        result.push_str(&html[..pos]);
        result.push_str(&styles);
        result.push_str(&html[pos..]);
        Ok(result)
    } else {
        Err(RheoError::HtmlGeneration {
            count: 1,
            errors: "HTML document does not contain a </head> element".to_string(),
        })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // DOM tests

    #[test]
    fn test_parse_html() {
        let html = "<html><head><title>Test</title></head><body></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        assert!(!dom.document_root().children.borrow().is_empty());
    }

    #[test]
    fn test_find_element() {
        let html = "<html><head><title>Test</title></head><body></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let head = dom.find_element("head");
        assert!(head.is_some());
        assert_eq!(head.unwrap().tag_name(), "head");
    }

    #[test]
    fn test_find_element_not_found() {
        let html = "<html><body></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let script = dom.find_element("script");
        assert!(script.is_none());
    }

    #[test]
    fn test_create_link_element() {
        let link = Element::create_link("stylesheet", "style.css");
        assert_eq!(link.tag_name(), "link");

        match &link.handle.data {
            NodeData::Element { name, attrs, .. } => {
                assert_eq!(name.local.as_ref(), "link");
                let attrs = attrs.borrow();
                assert_eq!(attrs.len(), 2);
                assert_eq!(attrs[0].name.local.as_ref(), "rel");
                assert_eq!(attrs[0].value.as_ref(), "stylesheet");
                assert_eq!(attrs[1].name.local.as_ref(), "href");
                assert_eq!(attrs[1].value.as_ref(), "style.css");
            }
            _ => panic!("expected Element node"),
        }
    }

    #[test]
    fn test_serialize_html() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let serialized = dom.serialize().unwrap();
        assert!(serialized.contains("<!DOCTYPE html>"));
        assert!(serialized.contains("<title>Test</title>"));
    }

    #[test]
    fn test_prepend_child() {
        let html = "<html><head><title>Test</title></head></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let head = dom.find_element("head").unwrap();

        let link = Element::create_link("stylesheet", "style.css");
        head.prepend_child(link);

        let serialized = dom.serialize().unwrap();
        assert!(serialized.contains("<link rel=\"stylesheet\" href=\"style.css\">"));
    }

    // inject_inline_styles tests

    #[test]
    fn test_inject_inline_styles_basic() {
        let html = "<!DOCTYPE html><html><head></head><body></body></html>";
        let result = inject_inline_styles(html, &["body { color: red; }"]).unwrap();
        assert!(result.contains("<style>body { color: red; }</style>"));
    }

    #[test]
    fn test_inject_inline_styles_empty() {
        let html = "<!DOCTYPE html><html><head></head><body></body></html>";
        let result = inject_inline_styles(html, &[]).unwrap();
        assert_eq!(result, html);
    }

    #[test]
    fn test_inject_inline_styles_no_head() {
        let html = "<html><body></body></html>";
        let result = inject_inline_styles(html, &["body {}"]);
        assert!(result.is_err());
    }

    // inject_head_links tests (via HtmlDom)

    #[test]
    fn test_inject_head_links_basic() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_head_links(&[], &["style.css"], &[]).unwrap();
        let result = dom.serialize().unwrap();

        assert!(result.contains("<head>"));
        assert!(result.contains("<title>Test</title>"));
        assert!(result.contains(r#"<link rel="stylesheet" href="style.css">"#));

        let head_pos = result.find("<head>").unwrap();
        let link_pos = result
            .find(r#"<link rel="stylesheet" href="style.css">"#)
            .unwrap();
        assert!(link_pos > head_pos);
    }

    #[test]
    fn test_inject_head_links_multiple_stylesheets() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_head_links(&[], &["style.css", "custom.css"], &[])
            .unwrap();
        let result = dom.serialize().unwrap();

        assert!(result.contains(r#"<link rel="stylesheet" href="style.css">"#));
        assert!(result.contains(r#"<link rel="stylesheet" href="custom.css">"#));

        let style_pos = result.find(r#"href="style.css"#).unwrap();
        let custom_pos = result.find(r#"href="custom.css"#).unwrap();
        assert!(style_pos < custom_pos);
    }

    #[test]
    fn test_inject_head_links_with_fonts() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let fonts = &["https://fonts.googleapis.com/css2?family=Inter"];
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_head_links(fonts, &["style.css"], &[]).unwrap();
        let result = dom.serialize().unwrap();

        assert!(result.contains(r#"<link rel="stylesheet" href="style.css">"#));
        assert!(result.contains(
            r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter">"#
        ));

        let font_pos = result.find(r#"fonts.googleapis.com"#).unwrap();
        let style_pos = result.find(r#"href="style.css"#).unwrap();
        assert!(font_pos < style_pos);
    }

    #[test]
    fn test_inject_head_links_preserves_existing_content() {
        let html = r#"<!DOCTYPE html>
<html>
<head>
<title>Test</title>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body></body>
</html>"#;
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_head_links(&[], &["style.css"], &[]).unwrap();
        let result = dom.serialize().unwrap();

        assert!(result.contains("<title>Test</title>"));
        assert!(result.contains(r#"<meta charset="UTF-8">"#));
        assert!(result.contains(r#"<meta name="viewport""#));
        assert!(result.contains(r#"<link rel="stylesheet" href="style.css">"#));

        let last_meta_pos = result.find(r#"<meta name="viewport""#).unwrap();
        let link_pos = result
            .find(r#"<link rel="stylesheet" href="style.css">"#)
            .unwrap();
        assert!(
            link_pos > last_meta_pos,
            "link should appear after meta tags"
        );
    }

    #[test]
    fn test_inject_head_links_no_head_element() {
        // html5ever automatically creates a <head> element per HTML5 spec
        let html = "<!DOCTYPE html><html><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        let result = dom.inject_head_links(&[], &["style.css"], &[]);

        assert!(result.is_ok());
        let html_output = dom.serialize().unwrap();
        assert!(html_output.contains(r#"<link rel="stylesheet" href="style.css">"#));
    }

    #[test]
    fn test_inject_head_links_empty_lists() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_head_links(&[], &[], &[]).unwrap();
        let result = dom.serialize().unwrap();

        assert!(result.contains("<title>Test</title>"));
        assert!(!result.contains(r#"<link rel="stylesheet""#));
    }

    #[test]
    fn test_inject_head_links_with_scripts() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_head_links(&[], &[], &["index.js"]).unwrap();
        let result = dom.serialize().unwrap();

        assert!(result.contains(r#"src="index.js""#));
        assert!(result.contains("defer"));
    }

    #[test]
    fn test_inject_head_links_scripts_with_stylesheets() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_head_links(&[], &["style.css"], &["index.js"])
            .unwrap();
        let result = dom.serialize().unwrap();

        assert!(result.contains(r#"src="index.js""#));
        assert!(result.contains("defer"));
        assert!(result.contains(r#"<link rel="stylesheet" href="style.css">"#));
    }

    #[test]
    fn test_inject_head_links_no_scripts() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_head_links(&[], &["style.css"], &[]).unwrap();
        let result = dom.serialize().unwrap();

        assert!(!result.contains("<script"));
    }

    // inject_feed_link tests (via HtmlDom)

    #[test]
    fn test_inject_feed_link_basic() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_feed_link("https://example.com/feed.xml", "My Feed")
            .unwrap();
        let result = dom.serialize().unwrap();

        assert!(result.contains(r#"<head>"#));
        assert!(result.contains(r#"type="application/atom+xml""#));
        assert!(result.contains(r#"rel="alternate""#));
        assert!(result.contains(r#"title="My Feed""#));
        assert!(result.contains(r#"href="https://example.com/feed.xml""#));
    }

    #[test]
    fn test_inject_feed_link_after_meta_tags() {
        let html = r#"<!DOCTYPE html><html><head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width">
<title>Test</title>
</head><body></body></html>"#;
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.inject_feed_link("/feed.xml", "Blog").unwrap();
        let result = dom.serialize().unwrap();

        let last_meta_pos = result.find(r#"<meta name="viewport""#).unwrap();
        let feed_link_pos = result.find("application/atom+xml").unwrap();
        assert!(
            feed_link_pos > last_meta_pos,
            "feed link should appear after meta tags"
        );
    }

    #[test]
    fn test_inject_feed_link_no_head() {
        let html = "<!DOCTYPE html><html><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        let result = dom.inject_feed_link("/feed.xml", "Blog");
        // html5ever creates a <head> automatically per HTML5 spec
        assert!(result.is_ok());
    }

    // body_inner_html tests (via HtmlDom)

    #[test]
    fn test_body_inner_html_basic() {
        let html = "<html><head></head><body><p>Hi</p></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.body_inner_html().unwrap();
        assert_eq!(inner, "<p>Hi</p>");
    }

    #[test]
    fn test_body_inner_html_multiple_children() {
        let html = "<html><head></head><body><h1>T</h1><p>Body</p></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.body_inner_html().unwrap();
        assert_eq!(inner, "<h1>T</h1><p>Body</p>");
    }

    #[test]
    fn test_body_inner_html_empty_body() {
        let html = "<html><head></head><body></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.body_inner_html().unwrap();
        assert_eq!(inner, "");
    }

    // feed_content_inner_html tests (via HtmlDom)

    #[test]
    fn test_feed_content_main_wins() {
        let html = "<html><head></head><body><main><p>article</p></main><footer>chrome</footer></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.feed_content_inner_html().unwrap();
        assert_eq!(inner, "<p>article</p>");
    }

    #[test]
    fn test_feed_content_class_fallback() {
        let html = "<html><head></head><body><div class=\"rheo-feed-content\"><p>a</p></div><nav>x</nav></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.feed_content_inner_html().unwrap();
        assert_eq!(inner, "<p>a</p>");
    }

    #[test]
    fn test_feed_content_class_among_many() {
        // Whitespace-token membership, not substring: a multi-class attribute matches.
        let html = "<html><head></head><body><div class=\"post rheo-feed-content wide\"><p>a</p></div></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.feed_content_inner_html().unwrap();
        assert_eq!(inner, "<p>a</p>");
    }

    #[test]
    fn test_feed_content_body_fallback() {
        let html = "<html><head></head><body><h1>T</h1><p>Body</p></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.feed_content_inner_html().unwrap();
        assert_eq!(inner, dom.body_inner_html().unwrap());
        assert_eq!(inner, "<h1>T</h1><p>Body</p>");
    }

    #[test]
    fn test_feed_content_main_precedence_over_class() {
        let html = "<html><head></head><body><main><p>main</p></main><div class=\"rheo-feed-content\"><p>class</p></div></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.feed_content_inner_html().unwrap();
        assert_eq!(inner, "<p>main</p>");
    }
}

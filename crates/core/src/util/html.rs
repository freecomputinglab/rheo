//! HTML utilities for parsing, manipulating, and serializing HTML documents.
//!
//! Provides DOM manipulation via html5ever and head-injection helpers used by
//! the html plugin and any other crate that needs to post-process HTML output.

use crate::{Result, RheoError};
use html5ever::{
    Attribute, LocalName, ParseOpts, QualName, ns,
    tendril::{StrTendril, TendrilSink},
};
use markup5ever_rcdom::{Handle, Node, NodeData, RcDom};
use std::cell::RefCell;
use std::fmt::Write as _;

// ─── Serialization mode and helpers ──────────────────────────────────────────

/// Controls how the DOM is serialized.
pub enum SerializeMode {
    /// Standard HTML: void elements emit `<tag>`.
    Html,
    /// XHTML: void elements self-close `<tag/>`, attribute values fully escaped.
    Xhtml,
}

/// Returns true if the given tag name is an HTML raw-text element.
///
/// Raw-text elements (`style`, `script`) must have their text content serialized
/// verbatim — no entity escaping. CSS selectors like `p > span` would otherwise
/// become `p &gt; span`.
pub fn is_raw_text_element(tag: &str) -> bool {
    matches!(tag, "style" | "script")
}

/// Returns true if the given tag name is an HTML void element.
pub fn is_void_element(tag_name: &str) -> bool {
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

/// Escape text content for HTML/XHTML output.
pub fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Escape an attribute value for HTML/XHTML output.
pub fn escape_attr(value: &str, mode: &SerializeMode) -> String {
    let escaped = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    match mode {
        SerializeMode::Html => escaped,
        SerializeMode::Xhtml => escaped.replace('"', "&quot;"),
    }
}

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
        serialize_node(&self.dom.document, &mut output, &SerializeMode::Html)?;
        Ok(output)
    }

    /// Find an element by tag name (depth-first search).
    pub fn find_element(&self, tag_name: &str) -> Option<Element> {
        find_element_by_tag(&self.dom.document, tag_name).map(|handle| Element { handle })
    }

    /// Hoists every `<rheo-head>` wrapper element's children into `<head>`,
    /// removing the (now-empty) wrapper from wherever in the body it appears.
    ///
    /// This is a general escape hatch for putting arbitrary elements into a
    /// page's own `<head>` from authored Typst or an imported package, neither
    /// of which can otherwise reach `<head>` (Typst only ever builds `<head>`
    /// from `DocumentInfo`). `html.elem("rheo-head", html.elem("meta", ..))`
    /// anywhere in the body moves that `<meta>` into `<head>` and leaves no
    /// `<rheo-head>` trace in the output. Multiple wrappers are all hoisted,
    /// in the document order they appear in the body; an empty
    /// `<rheo-head></rheo-head>` simply disappears, contributing nothing.
    ///
    /// Hoisted children are appended to `<head>` after everything already
    /// there (native Typst metadata, `inject_head_links`'s stylesheets/
    /// scripts), in the order the wrappers were collected — author-supplied
    /// content lands last rather than splicing ahead of rheo's own head
    /// management.
    ///
    /// If no `<rheo-head>` element exists anywhere in the document, this is a
    /// complete no-op: the document serializes byte-identical to before the
    /// call (and `<head>` need not even exist in that case).
    ///
    /// HTML-plugin only: EPUB output (`crates/epub`) never calls this, so a
    /// `<rheo-head>` wrapper left in an EPUB page passes through untouched.
    pub fn hoist_rheo_head(&mut self) -> Result<()> {
        let mut collected = Vec::new();
        hoist_rheo_head_children(&self.dom.document, &mut collected);

        if collected.is_empty() {
            return Ok(());
        }

        let head = self
            .find_element("head")
            .ok_or_else(|| RheoError::HtmlGeneration {
                count: 1,
                errors: "HTML document does not contain a <head> element".to_string(),
            })?;

        for handle in collected {
            head.append_child(Element { handle });
        }

        Ok(())
    }

    /// Appends the top-level elements of an HTML fragment into this
    /// document's own `<head>`, after whatever is already there — including
    /// anything [`Self::hoist_rheo_head`] already moved in for this page.
    ///
    /// This is the site-wide counterpart to `hoist_rheo_head`'s per-page
    /// escape hatch: a bundle-root `.rheo/head.html` control asset (see
    /// [`crate::transclude::ControlAssets`]) runs outside every page's own
    /// `#document`, so it cannot use a `<rheo-head>` wrapper at all — instead
    /// its whole decoded fragment is appended, once per page, via this
    /// method. Site-wide content intentionally lands *after* per-page
    /// `<rheo-head>` content, so an author's own page-level head content
    /// takes precedence in reading order over a project-wide default.
    ///
    /// `fragment_html` has no wrapping `<html>/<head>/<body>` — just the
    /// elements to append (e.g. `<link rel="alternate" ...><meta ...>`).
    /// html5ever only exposes a full-document parser
    /// ([`html5ever::parse_document`], used by [`Self::parse`]), not a
    /// fragment parser, so `fragment_html` is parsed by wrapping it in a
    /// minimal stub document (`<!DOCTYPE html><html><head>{fragment}</head>
    /// <body></body></html>`); the stub's own `<head>` children are then
    /// moved, in order, into this document's real `<head>`.
    pub fn append_head_fragment(&mut self, fragment_html: &str) -> Result<()> {
        let stub = format!("<!DOCTYPE html><html><head>{fragment_html}</head><body></body></html>");
        let stub_dom = Self::parse(&stub)?;
        let stub_head = stub_dom
            .find_element("head")
            .ok_or_else(|| RheoError::HtmlGeneration {
                count: 1,
                errors: "failed to parse head fragment: stub document has no <head>".to_string(),
            })?;

        let head = self
            .find_element("head")
            .ok_or_else(|| RheoError::HtmlGeneration {
                count: 1,
                errors: "HTML document does not contain a <head> element".to_string(),
            })?;

        for child in stub_head.take_children() {
            head.append_child(child);
        }

        Ok(())
    }

    /// Inject `<link>` and `<script>` elements into the HTML `<head>`.
    ///
    /// Refs are inserted verbatim, so callers that link a build-root asset from a
    /// page in a subdirectory must first make each ref depth-relative — see
    /// [`depth_prefix`].
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

    /// Serialize the inner HTML of a selected region.
    ///
    /// `select: None` uses the default cascade, first match wins:
    /// 1. the first `<main>` element's inner HTML;
    /// 2. the first element carrying the `rheo-content` class;
    /// 3. the whole `<body>` inner HTML.
    ///
    /// This lets authors scope a region — e.g. a transcluded article, an
    /// aggregator's excerpt — excluding page chrome (header/footer/nav), by
    /// wrapping it in `<main>` (or an element with class `rheo-content`) and
    /// keeping the chrome outside it. With no marker present it falls back to
    /// the full body.
    ///
    /// `select: Some(tag)` selects the first element with that bare tag name
    /// instead (e.g. `"article"`). `select: Some(".class")` (a leading dot)
    /// selects the first element carrying `class` as a whitespace-separated
    /// token instead (e.g. `".rheo-content"`).
    ///
    /// Returns an error if an explicit `select` matches no element, or (in the
    /// default cascade's body fallback) if the document has no `<body>`.
    pub fn select_inner_html(&self, select: Option<&str>) -> Result<String> {
        match select {
            None => {
                if let Some(main) = find_element_by_tag(&self.dom.document, "main") {
                    return inner_html(&main);
                }
                if let Some(el) = find_element_by_class(&self.dom.document, "rheo-content") {
                    return inner_html(&el);
                }
                self.body_inner_html()
            }
            Some(sel) => {
                if let Some(class) = sel.strip_prefix('.') {
                    let el = find_element_by_class(&self.dom.document, class).ok_or_else(|| {
                        RheoError::HtmlGeneration {
                            count: 1,
                            errors: format!("no element with class '{}' found", class),
                        }
                    })?;
                    inner_html(&el)
                } else {
                    let el = find_element_by_tag(&self.dom.document, sel).ok_or_else(|| {
                        RheoError::HtmlGeneration {
                            count: 1,
                            errors: format!("no <{}> element found", sel),
                        }
                    })?;
                    inner_html(&el)
                }
            }
        }
    }

    /// Returns a reference to the underlying DOM document root node.
    pub fn document_root(&self) -> &Handle {
        &self.dom.document
    }
}

/// Wrapper around html5ever's Handle for type-safe element manipulation.
pub struct Element {
    handle: Handle,
}

impl Element {
    /// Create a DOM element node with the given tag and attribute pairs.
    fn create_element(tag: &str, attrs: &[(&str, &str)]) -> Self {
        let attrs: Vec<_> = attrs
            .iter()
            .map(|(k, v)| Attribute {
                name: QualName::new(None, ns!(), LocalName::from(*k)),
                value: StrTendril::from(*v),
            })
            .collect();

        let handle = Node::new(NodeData::Element {
            name: QualName::new(None, ns!(html), LocalName::from(tag)),
            attrs: RefCell::new(attrs),
            template_contents: RefCell::new(None),
            mathml_annotation_xml_integration_point: false,
        });

        Self { handle }
    }

    /// Create a `<link rel="..." href="...">` element.
    pub fn create_link(rel: &str, href: &str) -> Self {
        Self::create_element("link", &[("rel", rel), ("href", href)])
    }

    /// Create a `<script src="..."></script>` element.
    pub fn create_script(src: &str) -> Self {
        Self::create_element("script", &[("src", src), ("defer", "")])
    }

    /// Prepend a child element to this element.
    pub fn prepend_child(&self, child: Element) {
        let mut children = self.handle.children.borrow_mut();
        children.insert(0, child.handle);
    }

    /// Append a child element to this element.
    pub fn append_child(&self, child: Element) {
        let mut children = self.handle.children.borrow_mut();
        children.push(child.handle);
    }

    /// Insert a child element at the given index (clamped to children length).
    pub fn insert_child_at(&self, index: usize, child: Element) {
        let mut children = self.handle.children.borrow_mut();
        let index = index.min(children.len());
        children.insert(index, child.handle);
    }

    /// Take this element's children, leaving it with none. Used to move a
    /// parsed fragment's top-level nodes into another document's tree (see
    /// [`HtmlDom::append_head_fragment`]).
    fn take_children(&self) -> Vec<Element> {
        std::mem::take(&mut *self.handle.children.borrow_mut())
            .into_iter()
            .map(|handle| Element { handle })
            .collect()
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

/// Rewrites `handle`'s own children in place: any child that is itself a
/// `<rheo-head>` element is removed from the list and its children are
/// appended, in order, to `collected`; every remaining child is recursed
/// into first (so a wrapper nested deeper in the tree, or appearing later in
/// a sibling list, is still found) before being kept. Walking children in
/// their existing left-to-right order and recursing into each before moving
/// to the next preserves document order in `collected` across multiple
/// wrappers anywhere in the tree.
fn hoist_rheo_head_children(handle: &Handle, collected: &mut Vec<Handle>) {
    let old_children = std::mem::take(&mut *handle.children.borrow_mut());
    let mut new_children = Vec::with_capacity(old_children.len());

    for child in old_children {
        let is_wrapper = matches!(&child.data, NodeData::Element { name, .. } if name.local.as_ref() == "rheo-head");

        if is_wrapper {
            // Recurse into the wrapper first in case it contains a nested
            // `<rheo-head>` of its own, then move its (now-cleaned) children
            // into the accumulator. The wrapper itself is dropped.
            hoist_rheo_head_children(&child, collected);
            let wrapper_children = std::mem::take(&mut *child.children.borrow_mut());
            collected.extend(wrapper_children);
        } else {
            hoist_rheo_head_children(&child, collected);
            new_children.push(child);
        }
    }

    *handle.children.borrow_mut() = new_children;
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
        serialize_node(child, &mut output, &SerializeMode::Html)?;
    }
    Ok(output)
}

fn serialize_node(handle: &Handle, output: &mut String, mode: &SerializeMode) -> Result<()> {
    match &handle.data {
        NodeData::Document => {
            for child in handle.children.borrow().iter() {
                serialize_node(child, output, mode)?;
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
            output.push_str(&escape_text(&text));
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
                let escaped_value = escape_attr(&attr.value, mode);
                write!(output, " {}=\"{}\"", attr.name.local, escaped_value).map_err(|e| {
                    RheoError::HtmlGeneration {
                        count: 1,
                        errors: format!("failed to serialize attribute: {}", e),
                    }
                })?;
            }

            if is_void_element(&name.local) {
                match mode {
                    SerializeMode::Xhtml => output.push_str("/>"),
                    SerializeMode::Html => output.push('>'),
                }
            } else {
                output.push('>');
                let is_raw =
                    matches!(mode, SerializeMode::Html) && is_raw_text_element(name.local.as_ref());
                for child in handle.children.borrow().iter() {
                    if is_raw && let NodeData::Text { contents } = &child.data {
                        output.push_str(&contents.borrow());
                    } else {
                        serialize_node(child, output, mode)?;
                    }
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

/// The `../` prefix that makes a build-root asset ref resolve from a page at the
/// given output path. `index.html` → `""`; `chapters/ch1.html` → `"../"`;
/// `a/b/c.html` → `"../../"`. Assets are written relative to the plugin output
/// root, so a page one directory deep must climb one level to reach them.
pub fn depth_prefix(output_rel_path: &str) -> String {
    "../".repeat(output_rel_path.matches('/').count())
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

    #[test]
    fn test_depth_prefix() {
        assert_eq!(depth_prefix("index.html"), "");
        assert_eq!(depth_prefix("chapters/ch1.html"), "../");
        assert_eq!(depth_prefix("a/b/c.html"), "../../");
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

    // hoist_rheo_head tests (via HtmlDom)

    #[test]
    fn test_hoist_rheo_head_single_wrapper() {
        let html = r#"<!DOCTYPE html><html><head><title>Test</title></head><body>
<p>Before</p>
<rheo-head><link rel="canonical" href="/a"></rheo-head>
<p>After</p>
</body></html>"#;
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.hoist_rheo_head().unwrap();
        let result = dom.serialize().unwrap();

        assert!(!result.contains("rheo-head"));
        let head_pos = result.find("<head>").unwrap();
        let head_end = result.find("</head>").unwrap();
        let link_pos = result.find(r#"<link rel="canonical" href="/a">"#).unwrap();
        assert!(
            link_pos > head_pos && link_pos < head_end,
            "canonical link should be inside <head>"
        );
    }

    #[test]
    fn test_hoist_rheo_head_multiple_wrappers_preserve_order() {
        let html = r#"<!DOCTYPE html><html><head><title>Test</title></head><body>
<p>First</p>
<rheo-head><meta name="b-first" content="one"></rheo-head>
<p>Middle</p>
<rheo-head><meta name="b-second" content="two"></rheo-head>
<p>Last</p>
</body></html>"#;
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.hoist_rheo_head().unwrap();
        let result = dom.serialize().unwrap();

        assert!(!result.contains("rheo-head"));
        let head_end = result.find("</head>").unwrap();
        let first_pos = result.find(r#"name="b-first""#).unwrap();
        let second_pos = result.find(r#"name="b-second""#).unwrap();
        assert!(first_pos < head_end && second_pos < head_end);
        assert!(
            first_pos < second_pos,
            "hoisted metas must land in <head> in the same order the wrappers appeared in the body"
        );
    }

    #[test]
    fn test_hoist_rheo_head_no_wrapper_is_noop() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body><p>Just text</p></body></html>";
        let dom_before = HtmlDom::parse(html).unwrap();
        let before = dom_before.serialize().unwrap();

        let mut dom_after = HtmlDom::parse(html).unwrap();
        dom_after.hoist_rheo_head().unwrap();
        let after = dom_after.serialize().unwrap();

        assert_eq!(
            before, after,
            "hoist_rheo_head must be a no-op with no <rheo-head>"
        );
    }

    // append_head_fragment tests (via HtmlDom)

    #[test]
    fn test_append_head_fragment_appends_top_level_elements() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.append_head_fragment(
            r#"<link rel="canonical" href="/page"><meta name="site-wide" content="yes">"#,
        )
        .unwrap();
        let result = dom.serialize().unwrap();

        assert!(result.contains("<title>Test</title>"));
        assert!(result.contains(r#"<link rel="canonical" href="/page">"#));
        assert!(result.contains(r#"<meta name="site-wide" content="yes">"#));

        let head_pos = result.find("<head>").unwrap();
        let head_end = result.find("</head>").unwrap();
        let link_pos = result.find(r#"rel="canonical""#).unwrap();
        let meta_pos = result.find(r#"name="site-wide""#).unwrap();
        assert!(link_pos > head_pos && link_pos < head_end);
        assert!(meta_pos > head_pos && meta_pos < head_end);
        assert!(
            link_pos < meta_pos,
            "fragment's own top-level elements must land in their original order"
        );
    }

    #[test]
    fn test_append_head_fragment_lands_after_existing_head_content() {
        let html = r#"<!DOCTYPE html><html><head><title>Test</title><meta charset="UTF-8"></head><body></body></html>"#;
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.append_head_fragment(r#"<meta name="site-wide" content="yes">"#)
            .unwrap();
        let result = dom.serialize().unwrap();

        let charset_pos = result.find(r#"charset="UTF-8""#).unwrap();
        let site_wide_pos = result.find(r#"name="site-wide""#).unwrap();
        assert!(
            site_wide_pos > charset_pos,
            "site-wide fragment must land after existing head content"
        );
    }

    #[test]
    fn test_append_head_fragment_lands_after_hoisted_rheo_head() {
        let html = r#"<!DOCTYPE html><html><head><title>Test</title></head><body>
<rheo-head><meta name="per-page" content="yes"></rheo-head>
</body></html>"#;
        let mut dom = HtmlDom::parse(html).unwrap();
        dom.hoist_rheo_head().unwrap();
        dom.append_head_fragment(r#"<meta name="site-wide" content="yes">"#)
            .unwrap();
        let result = dom.serialize().unwrap();

        let per_page_pos = result.find(r#"name="per-page""#).unwrap();
        let site_wide_pos = result.find(r#"name="site-wide""#).unwrap();
        assert!(
            per_page_pos < site_wide_pos,
            "site-wide head content must land after per-page <rheo-head> content"
        );
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

    // select_inner_html tests (via HtmlDom)

    #[test]
    fn test_select_inner_html_default_main_wins() {
        let html = "<html><head></head><body><main><p>article</p></main><footer>chrome</footer></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.select_inner_html(None).unwrap();
        assert_eq!(inner, "<p>article</p>");
    }

    #[test]
    fn test_select_inner_html_default_class_fallback() {
        let html = "<html><head></head><body><div class=\"rheo-content\"><p>a</p></div><nav>x</nav></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.select_inner_html(None).unwrap();
        assert_eq!(inner, "<p>a</p>");
    }

    #[test]
    fn test_select_inner_html_default_class_among_many() {
        // Whitespace-token membership, not substring: a multi-class attribute matches.
        let html = "<html><head></head><body><div class=\"post rheo-content wide\"><p>a</p></div></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.select_inner_html(None).unwrap();
        assert_eq!(inner, "<p>a</p>");
    }

    #[test]
    fn test_select_inner_html_default_body_fallback() {
        let html = "<html><head></head><body><h1>T</h1><p>Body</p></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.select_inner_html(None).unwrap();
        assert_eq!(inner, dom.body_inner_html().unwrap());
        assert_eq!(inner, "<h1>T</h1><p>Body</p>");
    }

    #[test]
    fn test_select_inner_html_default_main_precedence_over_class() {
        let html = "<html><head></head><body><main><p>main</p></main><div class=\"rheo-content\"><p>class</p></div></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.select_inner_html(None).unwrap();
        assert_eq!(inner, "<p>main</p>");
    }

    #[test]
    fn test_select_inner_html_by_bare_tag() {
        let html = "<html><head></head><body><article><p>a</p></article><main><p>main</p></main></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.select_inner_html(Some("article")).unwrap();
        assert_eq!(inner, "<p>a</p>");
    }

    #[test]
    fn test_select_inner_html_by_class() {
        let html = "<html><head></head><body><main><p>main</p></main><div class=\"custom\"><p>c</p></div></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let inner = dom.select_inner_html(Some(".custom")).unwrap();
        assert_eq!(inner, "<p>c</p>");
    }

    #[test]
    fn test_select_inner_html_missing_tag_errors() {
        let html = "<html><head></head><body><p>x</p></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let result = dom.select_inner_html(Some("article"));
        assert!(result.is_err());
    }

    #[test]
    fn test_select_inner_html_missing_class_errors() {
        let html = "<html><head></head><body><p>x</p></body></html>";
        let dom = HtmlDom::parse(html).unwrap();
        let result = dom.select_inner_html(Some(".missing"));
        assert!(result.is_err());
    }
}

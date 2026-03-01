//! DOM manipulation utilities using html5ever.
//!
//! This module provides shared functionality for parsing, manipulating, and
//! serializing HTML/XHTML using the html5ever parser.

use crate::{Result, RheoError};
use html5ever::{ParseOpts, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::fmt::Write as _;

/// Wrapper around html5ever's RcDom for type-safe DOM manipulation.
///
/// Provides methods for parsing HTML, finding elements, and serializing back to HTML.
pub struct HtmlDom {
    dom: RcDom,
}

impl HtmlDom {
    /// Parse HTML string into a DOM tree.
    ///
    /// # Arguments
    /// * `html` - The HTML content to parse
    ///
    /// # Returns
    /// Parsed DOM wrapped in HtmlDom
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
    ///
    /// # Returns
    /// Serialized HTML string
    pub fn serialize(&self) -> Result<String> {
        let mut output = String::new();
        serialize_node(&self.dom.document, &mut output)?;
        Ok(output)
    }

    /// Find an element by tag name (depth-first search).
    ///
    /// # Arguments
    /// * `tag_name` - The tag name to search for (e.g., "head", "body")
    ///
    /// # Returns
    /// Element wrapper, or None if not found
    pub fn find_element(&self, tag_name: &str) -> Option<Element> {
        find_element_by_tag(&self.dom.document, tag_name).map(|handle| Element { handle })
    }

    /// Get the document root handle.
    ///
    /// # Returns
    /// Reference to the document root handle
    pub fn document_root(&self) -> &Handle {
        &self.dom.document
    }
}

/// Wrapper around html5ever's Handle for type-safe element manipulation.
///
/// Represents an HTML element node in the DOM tree.
pub struct Element {
    handle: Handle,
}

impl Element {
    /// Create a link element with the specified rel and href attributes.
    ///
    /// # Arguments
    /// * `rel` - The rel attribute value (e.g., "stylesheet")
    /// * `href` - The href attribute value (e.g., "style.css")
    ///
    /// # Returns
    /// New Element containing a link node
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

    /// Prepend a child element to this element.
    ///
    /// # Arguments
    /// * `child` - The child element to prepend
    pub fn prepend_child(&self, child: Element) {
        let mut children = self.handle.children.borrow_mut();
        children.insert(0, child.handle);
    }

    /// Get the tag name of this element.
    ///
    /// # Returns
    /// Tag name as a string slice, or empty string if not an element
    pub fn tag_name(&self) -> &str {
        match &self.handle.data {
            NodeData::Element { name, .. } => name.local.as_ref(),
            _ => "",
        }
    }
}

/// Find an element by tag name in the DOM tree (depth-first search).
fn find_element_by_tag(handle: &Handle, tag_name: &str) -> Option<Handle> {
    match &handle.data {
        NodeData::Element { name, .. } if name.local.as_ref() == tag_name => {
            return Some(handle.clone());
        }
        _ => {}
    }

    // Search children recursively
    for child in handle.children.borrow().iter() {
        if let Some(found) = find_element_by_tag(child, tag_name) {
            return Some(found);
        }
    }

    None
}

/// Serialize a single node and its children to HTML.
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
            // Escape text content
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

            // Check for self-closing tags
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

/// Check if an element is a void element (self-closing).
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

#[cfg(test)]
mod tests {
    use super::*;

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

        // Verify attributes by checking the handle's node data
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
}

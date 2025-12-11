//! DOM manipulation utilities using html5ever.
//!
//! This module provides shared functionality for parsing, manipulating, and
//! serializing HTML/XHTML using the html5ever parser.

use crate::{Result, RheoError};
use html5ever::{tendril::TendrilSink, ParseOpts};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::fmt::Write as _;

/// Parse HTML string into a DOM tree using html5ever.
///
/// # Arguments
/// * `html` - The HTML content to parse
///
/// # Returns
/// Parsed DOM tree as RcDom
pub fn parse_html(html: &str) -> Result<RcDom> {
    let dom = html5ever::parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .map_err(|e| RheoError::HtmlGeneration {
            count: 1,
            errors: format!("failed to parse HTML: {}", e),
        })?;
    Ok(dom)
}

/// Serialize a DOM tree back to an HTML string.
///
/// # Arguments
/// * `dom` - The parsed DOM tree
///
/// # Returns
/// Serialized HTML string
pub fn serialize_html(dom: &RcDom) -> Result<String> {
    let mut output = String::new();
    serialize_node(&dom.document, &mut output)?;
    Ok(output)
}

/// Find an element by tag name in the DOM tree (depth-first search).
///
/// # Arguments
/// * `handle` - The node to start searching from
/// * `tag_name` - The tag name to search for (e.g., "head", "body")
///
/// # Returns
/// Handle to the first matching element, or None if not found
pub fn find_element_by_tag(handle: &Handle, tag_name: &str) -> Option<Handle> {
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

/// Create a link element node with the specified rel and href attributes.
///
/// # Arguments
/// * `rel` - The rel attribute value (e.g., "stylesheet")
/// * `href` - The href attribute value (e.g., "style.css")
///
/// # Returns
/// Handle to the newly created link element
pub fn create_link_element(rel: &str, href: &str) -> Handle {
    use html5ever::tendril::StrTendril;
    use html5ever::{ns, Attribute, LocalName, QualName};
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

    let node = Node::new(NodeData::Element {
        name: QualName::new(None, ns!(html), LocalName::from("link")),
        attrs: RefCell::new(attrs),
        template_contents: RefCell::new(None),
        mathml_annotation_xml_integration_point: false,
    });

    node
}

/// Prepend a child node to a parent node.
///
/// # Arguments
/// * `parent` - The parent node
/// * `child` - The child node to prepend
pub fn prepend_child(parent: &Handle, child: Handle) {
    let mut children = parent.children.borrow_mut();
    children.insert(0, child);
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
                write!(output, " {}=\"{}\"", attr.name.local, attr.value)
                    .map_err(|e| RheoError::HtmlGeneration {
                        count: 1,
                        errors: format!("failed to serialize attribute: {}", e),
                    })?;
            }

            // Check for self-closing tags
            if is_void_element(&name.local) {
                output.push_str(">");
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
        let dom = parse_html(html).unwrap();
        assert!(dom.document.children.borrow().len() > 0);
    }

    #[test]
    fn test_find_element_by_tag() {
        let html = "<html><head><title>Test</title></head><body></body></html>";
        let dom = parse_html(html).unwrap();
        let head = find_element_by_tag(&dom.document, "head");
        assert!(head.is_some());
    }

    #[test]
    fn test_find_element_by_tag_not_found() {
        let html = "<html><body></body></html>";
        let dom = parse_html(html).unwrap();
        let script = find_element_by_tag(&dom.document, "script");
        assert!(script.is_none());
    }

    #[test]
    fn test_create_link_element() {
        let link = create_link_element("stylesheet", "style.css");
        match &link.data {
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
        let dom = parse_html(html).unwrap();
        let serialized = serialize_html(&dom).unwrap();
        assert!(serialized.contains("<!DOCTYPE html>"));
        assert!(serialized.contains("<title>Test</title>"));
    }

    #[test]
    fn test_prepend_child() {
        let html = "<html><head><title>Test</title></head></html>";
        let dom = parse_html(html).unwrap();
        let head = find_element_by_tag(&dom.document, "head").unwrap();

        let link = create_link_element("stylesheet", "style.css");
        prepend_child(&head, link);

        let serialized = serialize_html(&dom).unwrap();
        assert!(serialized.contains("<link rel=\"stylesheet\" href=\"style.css\">"));
    }
}

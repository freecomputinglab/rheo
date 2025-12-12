//! HTML head injection utilities for CSS and font links.
//!
//! This module provides functionality for injecting stylesheet and font links
//! into the <head> section of HTML documents.

use crate::{Result, RheoError};

use super::dom;

/// Inject CSS and font links into HTML <head> section.
///
/// Parses the HTML, finds the <head> element, and prepends link elements for
/// stylesheets and fonts. Links are inserted in the order: fonts first, then stylesheets.
///
/// # Arguments
/// * `html` - The HTML content to modify
/// * `stylesheets` - Stylesheet paths to inject (e.g., ["style.css"])
/// * `fonts` - Font URLs to inject (e.g., ["https://fonts.googleapis.com/..."])
///
/// # Returns
/// HTML with injected links in the <head> section
///
/// # Errors
/// Returns error if:
/// - HTML parsing fails
/// - <head> element is not found
/// - HTML serialization fails
pub fn inject_head_links(
    html: &str,
    stylesheets: &[&str],
    fonts: &[&str],
) -> Result<String> {
    // Parse the HTML document
    let dom = dom::parse_html(html)?;

    // Find the <head> element
    let head = dom::find_element_by_tag(&dom.document, "head").ok_or_else(|| {
        RheoError::HtmlGeneration {
            count: 1,
            errors: "HTML document does not contain a <head> element".to_string(),
        }
    })?;

    // Create link elements for stylesheets and fonts
    // We prepend in reverse order so they end up in the correct order:
    // fonts first, then stylesheets

    // First prepend stylesheets (in reverse order)
    for stylesheet in stylesheets.iter().rev() {
        let link = dom::create_link_element("stylesheet", stylesheet);
        dom::prepend_child(&head, link);
    }

    // Then prepend fonts (in reverse order)
    for font in fonts.iter().rev() {
        let link = dom::create_link_element("stylesheet", font);
        dom::prepend_child(&head, link);
    }

    // Serialize the modified DOM back to HTML
    dom::serialize_html(&dom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_head_links_basic() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let result = inject_head_links(html, &["style.css"], &[]).unwrap();

        assert!(result.contains("<head>"));
        assert!(result.contains("<title>Test</title>"));
        assert!(result.contains(r#"<link rel="stylesheet" href="style.css">"#));

        // Verify stylesheet comes after <head> opening tag
        let head_pos = result.find("<head>").unwrap();
        let link_pos = result.find(r#"<link rel="stylesheet" href="style.css">"#).unwrap();
        assert!(link_pos > head_pos);
    }

    #[test]
    fn test_inject_head_links_multiple_stylesheets() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let result = inject_head_links(html, &["style.css", "custom.css"], &[]).unwrap();

        assert!(result.contains(r#"<link rel="stylesheet" href="style.css">"#));
        assert!(result.contains(r#"<link rel="stylesheet" href="custom.css">"#));

        // Verify order: style.css should come before custom.css
        let style_pos = result.find(r#"href="style.css"#).unwrap();
        let custom_pos = result.find(r#"href="custom.css"#).unwrap();
        assert!(style_pos < custom_pos);
    }

    #[test]
    fn test_inject_head_links_with_fonts() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let fonts = &["https://fonts.googleapis.com/css2?family=Inter"];
        let result = inject_head_links(html, &["style.css"], fonts).unwrap();

        assert!(result.contains(r#"<link rel="stylesheet" href="style.css">"#));
        assert!(result.contains(r#"<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter">"#));

        // Verify order: fonts should come before stylesheets
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
        let result = inject_head_links(html, &["style.css"], &[]).unwrap();

        // Verify all original content is preserved
        assert!(result.contains("<title>Test</title>"));
        assert!(result.contains(r#"<meta charset="UTF-8">"#));
        assert!(result.contains(r#"<meta name="viewport""#));

        // Verify new link is added
        assert!(result.contains(r#"<link rel="stylesheet" href="style.css">"#));
    }

    #[test]
    fn test_inject_head_links_no_head_element() {
        // Note: html5ever automatically creates a <head> element per HTML5 spec
        // even if it's not in the source HTML. This is correct behavior.
        // So this test verifies that injection works even with minimal HTML.
        let html = "<!DOCTYPE html><html><body></body></html>";
        let result = inject_head_links(html, &["style.css"], &[]);

        // Should succeed because html5ever creates the <head> element
        assert!(result.is_ok());
        let html_output = result.unwrap();
        assert!(html_output.contains(r#"<link rel="stylesheet" href="style.css">"#));
    }

    #[test]
    fn test_inject_head_links_empty_lists() {
        let html = "<!DOCTYPE html><html><head><title>Test</title></head><body></body></html>";
        let result = inject_head_links(html, &[], &[]).unwrap();

        // Should succeed but not add any links
        assert!(result.contains("<title>Test</title>"));
        assert!(!result.contains(r#"<link rel="stylesheet""#));
    }
}

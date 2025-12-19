/// File extension constants and shared regex patterns used throughout rheo

use lazy_static::lazy_static;
use regex::Regex;

// File extensions
pub const TYP_EXT: &str = ".typ";
pub const PDF_EXT: &str = ".pdf";
pub const HTML_EXT: &str = ".html";
pub const XHTML_EXT: &str = ".xhtml";
pub const EPUB_EXT: &str = ".epub";

// Regex patterns
lazy_static! {
    /// Pattern for Typst #link() syntax: #link("url")(body) or #link("url", body)
    pub static ref TYPST_LINK_PATTERN: Regex =
        Regex::new(r#"#link\("([^"]+)"\)(\[[^\]]+\]|,\s*[^)]+)"#)
            .expect("invalid TYPST_LINK_PATTERN");

    /// Pattern for HTML href attributes: href="url"
    pub static ref HTML_HREF_PATTERN: Regex =
        Regex::new(r#"href="([^"]*)""#)
            .expect("invalid HTML_HREF_PATTERN");

    /// Pattern for Typst label references: #label[text]
    pub static ref TYPST_LABEL_PATTERN: Regex =
        Regex::new(r"#\w+\[([^\]]+)\]")
            .expect("invalid TYPST_LABEL_PATTERN");
}

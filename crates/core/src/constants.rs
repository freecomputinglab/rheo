/// File extension constants and shared regex patterns used throughout rheo
use regex::Regex;
use std::sync::LazyLock;

// File extensions
pub const TYP_EXT: &str = ".typ";
pub const TYP_EXT_BARE: &str = "typ";
pub const PDF_EXT: &str = ".pdf";
pub const HTML_EXT: &str = ".html";
pub const XHTML_EXT: &str = ".xhtml";
pub const EPUB_EXT: &str = ".epub";

// Regex patterns

/// Pattern for Typst #link() syntax: #link("url")(body) or #link("url", body)
pub static TYPST_LINK_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"#link\("([^"]+)"\)(\[[^\]]+\]|,\s*[^)]+)"#).expect("invalid TYPST_LINK_PATTERN")
});

/// Pattern for HTML href attributes: href="url"
pub static HTML_HREF_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"href="([^"]*)""#).expect("invalid HTML_HREF_PATTERN"));

/// Pattern for Typst label references: #label[text]
pub static TYPST_LABEL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#\w+\[([^\]]+)\]").expect("invalid TYPST_LABEL_PATTERN"));

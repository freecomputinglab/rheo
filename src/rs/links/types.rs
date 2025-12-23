use std::ops::Range;
use typst::syntax::Span;

/// Information about a link extracted from the AST
#[derive(Debug, Clone)]
pub struct LinkInfo {
    /// The URL from the link (e.g., "./chapter2.typ")
    pub url: String,

    /// The body text of the link
    pub body: String,

    /// Source span for error reporting
    pub span: Span,

    /// Byte range in the source text
    pub byte_range: Range<usize>,
}

/// Link transformation operation
#[derive(Debug, Clone)]
pub enum LinkTransform {
    /// Remove link, keep only body text
    Remove { body: String },

    /// Replace URL with new value
    ReplaceUrl { new_url: String },

    /// Replace URL with label
    ReplaceUrlWithLabel { new_label: String },

    /// Keep original (no transformation)
    KeepOriginal,
}

/// A spine with relative linking tranformations
#[derive(Debug, Clone)]
pub struct RheoSpine {
    /// The name of the file or website that the spine will generate.
    pub title: String,

    /// Whether or not the source has been merged into a single file.
    /// This is only false in the case of HTML currently.
    pub is_merged: bool,

    /// Reticulated (relative link transformed) source files, always of length 1 if `is_merged`.
    pub source: Vec<String>,
}

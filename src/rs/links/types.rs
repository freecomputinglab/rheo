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

/// Output format determining link transformation behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Single-file PDF (strip links)
    PdfSingle,

    /// Merged PDF (convert to label references)
    PdfMerged,

    /// HTML output (.typ → .html)
    Html,

    /// EPUB output (.typ → .xhtml)
    Epub,
}

/// Link transformation operation
#[derive(Debug, Clone)]
pub enum LinkTransform {
    /// Remove link, keep only body text
    Remove { body: String },

    /// Replace URL with new value
    ReplaceUrl { new_url: String },

    /// Keep original (no transformation)
    KeepOriginal,
}

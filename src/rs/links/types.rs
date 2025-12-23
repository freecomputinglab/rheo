/// Represents a link found in a Typst document
#[derive(Debug, Clone)]
pub struct LinkInfo {
    // TODO: Add fields for link information
}

/// Output format for link transformations
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Pdf,
    Html,
    Epub,
}

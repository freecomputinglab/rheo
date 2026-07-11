/// Commonly used Typst types re-exported for plugin use.
///
/// This module re-exports specific Typst types that plugins commonly need
/// for document introspection (e.g., EPUB plugin querying headings).
/// Plugins should import these from rheo_core rather than directly from typst.
// Re-export document type for HTML-based output formats
pub use typst_html::HtmlDocument;

// Re-export diagnostic string types
pub use typst::diag::{EcoString, eco_format};

// Re-export container types
pub use typst::ecow::eco_vec;

// Re-export foundation types
pub use typst::foundations::{NativeElement, StyleChain};

// Re-export model types for document structure
pub use typst::model::{Document, HeadingElem, OutlineNode};

// Re-export introspection trait (needed to call query() on HtmlIntrospector)
pub use typst::introspection::Introspector;

// Re-export Content for type annotations in plugin closures
pub use typst::foundations::Content;

//! Stateless helper modules shared across the crate.
//!
//! Open question: `html` is large and HTML-specific but remains a pure
//! helper; it lives here rather than in the html plugin for now.

pub mod constants;
pub mod html;
pub mod path;
pub mod pdf;
pub mod typst_literal;
pub mod typst_source;
pub mod typst_types;

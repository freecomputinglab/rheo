//! Link transformation (DEPRECATED)
//!
//! This module previously contained link rewriting logic for the old per-file
//! compilation path. The new bundle compilation path (VirtualSpine + Typst @ref)
//! handles cross-file references natively, making this module obsolete.
//!
//! The `LinkTransformer` struct and its methods have been removed. Use
//! `VirtualSpine::build()` for spine resolution and rheo-var harvesting instead.

use crate::Result;

/// Output of a single source transformation (DEPRECATED).
///
/// This struct was used to return rewritten source plus harvested rheo-* vars.
/// With bundle compilation, rheo-vars are harvested in `VirtualSpine::build`
/// and sources are synthesized by `VirtualSpine::source()`.
#[deprecated(note = "Use VirtualSpine::build for rheo-var harvesting")]
pub struct TransformOutput {
    /// Rewritten source code.
    pub source: String,
    /// Harvested rheo-* variables.
    pub rheo_vars: Vec<crate::reticulate::types::RheoVar>,
}

/// Link transformer (DEPRECATED).
///
/// The new bundle compilation path handles cross-file references via Typst's @ref,
/// making link transformation unnecessary.
#[deprecated(note = "Use VirtualSpine for spine resolution")]
pub struct LinkTransformer;

impl LinkTransformer {
    /// Stub constructor (DEPRECATED).
    #[deprecated(note = "No replacement needed")]
    pub(crate) fn new(_format_name: &str) -> Self {
        Self
    }

    /// Stub with_strategy method (DEPRECATED).
    #[deprecated(note = "No replacement needed")]
    pub fn with_strategy(self, _strategy: &str) -> Self {
        self
    }

    /// Stub with_spine method (DEPRECATED).
    #[deprecated(note = "No replacement needed")]
    pub fn with_spine(self, _spine: Vec<std::path::PathBuf>) -> Self {
        self
    }

    /// Stub with_import_rewriting method (DEPRECATED).
    #[deprecated(note = "No replacement needed")]
    pub fn with_import_rewriting(self, _rewrite: bool) -> Self {
        self
    }

    /// Stub transform method (DEPRECATED).
    #[deprecated(note = "No replacement needed")]
    pub fn transform_source(
        &self,
        _source: &str,
        _current_file: &std::path::Path,
        _project_root: &std::path::Path,
    ) -> Result<String> {
        Ok(String::new())
    }

    /// Stub transform_with_vars method (DEPRECATED).
    #[deprecated(note = "Use VirtualSpine::build for rheo-var harvesting")]
    pub fn transform_with_vars(
        &self,
        source: &str,
        _current_file: &std::path::Path,
        _project_root: &std::path::Path,
    ) -> Result<TransformOutput> {
        use crate::reticulate::parser;
        let source_obj = typst::syntax::Source::detached(source);
        let extracted = parser::extract_nodes(&source_obj);
        Ok(TransformOutput {
            source: String::new(),
            rheo_vars: extracted.rheo_vars,
        })
    }
}

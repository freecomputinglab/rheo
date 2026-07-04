//! Typst AST node extraction and analysis.
//!
//! This module extracts rheo variables and package imports from parsed Typst source.

use std::ops::Range;

/// Information about an import/include extracted from the AST (DEPRECATED).
///
/// The new bundle compilation path handles imports via Typst's native mechanisms,
/// making static import extraction obsolete for most cases. This type is retained for
/// package import extraction which is still needed.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The raw path string (e.g. "./utils.typ" or "@preview/foo:0.1.0")
    pub path: String,

    /// Byte range of the path string (NOT the whole statement)
    pub byte_range: Range<usize>,

    /// true if path starts with '@' (package import)
    pub is_package: bool,
}

/// A value bound to a `rheo-*` variable. String literals and booleans are
/// supported; the enum exists so further kinds (e.g. datetimes) can be added
/// without changing every consumer's signature.
#[derive(Debug, Clone, PartialEq)]
pub enum RheoValue {
    /// A string literal RHS.
    Str(String),
    /// A boolean literal RHS (`true`/`false`).
    Bool(bool),
}

impl RheoValue {
    /// The inner string if this is a [`RheoValue::Str`], else `None`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            RheoValue::Str(s) => Some(s),
            RheoValue::Bool(_) => None,
        }
    }

    /// The inner bool if this is a [`RheoValue::Bool`], else `None`.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            RheoValue::Bool(b) => Some(*b),
            RheoValue::Str(_) => None,
        }
    }
}

/// The `#set document(date: datetime(...))` timestamp harvested from a spine
/// vertebra during the canonical Typst parse.
///
/// Like [`RheoVar`], this is one element of the core Typst syntax decoded once at
/// parse time and threaded into downstream features (the HTML Atom feed). The
/// parsing lives in [`crate::reticulate::parser`] via the `FromSyntax` trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentDate(pub chrono::DateTime<chrono::Utc>);

/// A top-level `#let rheo-<key> = "..."` binding harvested from a spine
/// vertebra during the canonical Typst parse.
#[derive(Debug, Clone, PartialEq)]
pub struct RheoVar {
    /// The let-binding name with the leading `rheo-` prefix stripped
    /// (e.g. `rheo-feed-title` → `feed-title`).
    pub key: String,

    /// `Some(value)` when the RHS is a supported kind; `None` when it is any
    /// other kind. The consumer turns `None` into a validation error.
    pub value: Option<RheoValue>,

    /// 1-based source line of the binding, for error messages.
    pub line: usize,
}

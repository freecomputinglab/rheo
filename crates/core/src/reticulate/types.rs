//! Typst AST node extraction and analysis.
//!
//! This module extracts rheo variables and package imports from parsed Typst source.
//! Link extraction is deprecated — the new bundle compilation path handles cross-file
//! references via Typst @ref, making static link transformation obsolete.

use std::ops::Range;
use typst::syntax::Span;

/// Information about a link extracted from the AST (DEPRECATED).
///
/// The new bundle compilation path handles cross-file references via Typst @ref,
/// making static link extraction obsolete. This type is retained for backward compatibility
/// but link extraction functionality has been removed.
#[deprecated(note = "Use VirtualSpine + Typst @ref for cross-file references")]
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

    /// True when this link was extracted via a same-file wrapper function
    /// (e.g. `#let f(x) = link(x)`).  Wrapper-call byte ranges cover only
    /// the `Str` argument, not the full function call.
    pub is_wrapper_call: bool,
}

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

/// A value bound to a `rheo-*` variable. Currently only string literals are
/// supported; the enum exists so further kinds (e.g. datetimes) can be added
/// without changing every consumer's signature.
#[derive(Debug, Clone, PartialEq)]
pub enum RheoValue {
    /// A string literal RHS.
    Str(String),
}

impl RheoValue {
    /// The inner string if this is a [`RheoValue::Str`], else `None`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            RheoValue::Str(s) => Some(s),
        }
    }
}

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

//! Type-safe path handling utilities
//!
//! This module provides helper utilities for common path operations that would
//! otherwise require chained `unwrap()` calls. It ensures consistent error handling
//! for path-related operations throughout the codebase.

use crate::{Result, RheoError};
use std::path::{Path, PathBuf};

/// Convert a `Path` to a forward-slash string (cross-platform safe for Typst source).
pub fn to_forward_slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Escape text for use inside Typst square-bracket content `[…]`.
/// Escapes `\`, `[`, `]`, `#`.
pub fn escape_typst_content(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('#', "\\#")
}

/// Canonicalize a path, wrapping errors in RheoError.
pub fn canonicalize_path(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .map_err(|e| RheoError::path(path, format!("failed to canonicalize path: {}", e)))
}

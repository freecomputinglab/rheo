//! Type-safe path handling utilities
//!
//! This module provides helper utilities for common path operations that would
//! otherwise require chained `unwrap()` calls. It ensures consistent error handling
//! for path-related operations throughout the codebase.

use crate::{Result, RheoError};
use std::path::{Path, PathBuf};

/// Extension trait for Path to provide safe operations that return Result instead of Option
pub trait PathExt {
    /// Get file name as &str, returning error if None or non-UTF8
    ///
    /// # Errors
    /// Returns `RheoError::InvalidPath` if:
    /// - The path has no file name component
    /// - The file name is not valid UTF-8
    fn file_name_str(&self) -> Result<&str>;

    /// Get file stem (name without extension) as &str
    ///
    /// # Errors
    /// Returns `RheoError::InvalidPath` if:
    /// - The path has no file stem component
    /// - The file stem is not valid UTF-8
    fn file_stem_str(&self) -> Result<&str>;

    /// Get extension as &str
    ///
    /// # Errors
    /// Returns `RheoError::InvalidPath` if:
    /// - The path has no extension
    /// - The extension is not valid UTF-8
    fn extension_str(&self) -> Result<&str>;
}

impl PathExt for Path {
    fn file_name_str(&self) -> Result<&str> {
        self.file_name()
            .ok_or_else(|| RheoError::path(self, "path has no file name component"))?
            .to_str()
            .ok_or_else(|| RheoError::path(self, "file name contains invalid UTF-8"))
    }

    fn file_stem_str(&self) -> Result<&str> {
        self.file_stem()
            .ok_or_else(|| RheoError::path(self, "path has no file stem component"))?
            .to_str()
            .ok_or_else(|| RheoError::path(self, "file stem contains invalid UTF-8"))
    }

    fn extension_str(&self) -> Result<&str> {
        self.extension()
            .ok_or_else(|| RheoError::path(self, "path has no extension"))?
            .to_str()
            .ok_or_else(|| RheoError::path(self, "extension contains invalid UTF-8"))
    }
}

/// Convert a `Path` to a forward-slash string (cross-platform safe for Typst source).
pub fn to_forward_slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Sanitize one label segment: keep alphanumeric, `-`, `_`; replace everything
/// else with `_`. Safe for use in Typst label names.
pub fn sanitize_handle_segment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_file_name_str_success() {
        let path = PathBuf::from("/path/to/file.txt");
        assert_eq!(path.file_name_str().unwrap(), "file.txt");
    }

    #[test]
    fn test_file_name_str_no_filename() {
        let path = PathBuf::from("/");
        assert!(path.file_name_str().is_err());
    }

    #[test]
    fn test_file_stem_str_success() {
        let path = PathBuf::from("/path/to/file.txt");
        assert_eq!(path.file_stem_str().unwrap(), "file");
    }

    #[test]
    fn test_file_stem_str_no_stem() {
        let path = PathBuf::from("/");
        assert!(path.file_stem_str().is_err());
    }

    #[test]
    fn test_extension_str_success() {
        let path = PathBuf::from("/path/to/file.txt");
        assert_eq!(path.extension_str().unwrap(), "txt");
    }

    #[test]
    fn test_extension_str_no_extension() {
        let path = PathBuf::from("/path/to/file");
        assert!(path.extension_str().is_err());
    }
}

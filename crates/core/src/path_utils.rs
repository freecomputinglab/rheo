//! Type-safe path handling utilities
//!
//! This module provides helper utilities for common path operations that would
//! otherwise require chained `unwrap()` calls. It ensures consistent error handling
//! for path-related operations throughout the codebase.

use crate::{Result, RheoError, TYP_EXT_BARE};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

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

/// Single file discovery: require exactly one .typ file.
pub(crate) fn collect_one_typst_file(root: &Path) -> Result<Vec<PathBuf>> {
    let typst_files: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| Some(entry.ok()?.path().to_path_buf()))
        .filter(|entry| {
            entry
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == TYP_EXT_BARE)
                .unwrap_or(false)
        })
        .collect();

    match typst_files.len() {
        0 => Err(RheoError::project_config("need at least one .typ file")),
        1 => Ok(typst_files),
        _ => Err(RheoError::project_config(
            "multiple .typ files found, specify spine configuration",
        )),
    }
}

/// All .typ files discovery.
pub(crate) fn collect_all_typst_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut typst_files: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| Some(entry.ok()?.path().to_path_buf()))
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == TYP_EXT_BARE)
                .unwrap_or(false)
        })
        .collect();

    if typst_files.is_empty() {
        return Err(RheoError::project_config("need at least one .typ file"));
    }

    typst_files.sort();
    Ok(typst_files)
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

pub mod cli;
pub mod compile;
pub mod config;
pub mod error;
pub mod formats;
pub mod logging;
pub mod output;
pub mod project;
pub mod server;
pub mod spine;
pub mod watch;
pub mod world;

pub use cli::Cli;
pub use config::RheoConfig;
pub use error::RheoError;
pub use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fmt;
use std::path::Path;
use tracing::debug;

/// Result type alias using RheoError
pub type Result<T> = std::result::Result<T, RheoError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum OutputFormat {
    Html,
    Epub,
    Pdf,
}

impl OutputFormat {
    pub fn all_variants() -> Vec<Self> {
        vec![Self::Html, Self::Epub, Self::Pdf]
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl<'de> serde::Deserialize<'de> for OutputFormat {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "html" => Ok(OutputFormat::Html),
            "epub" => Ok(OutputFormat::Epub),
            "pdf" => Ok(OutputFormat::Pdf),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &["html", "epub", "pdf"],
            )),
        }
    }
}

/// Pattern type for FilterPatterns
#[derive(Debug, Clone)]
enum PatternType {
    /// Exclude pattern: files matching this are excluded
    Exclude(Glob),
    /// Include pattern: files must match at least one of these (if any exist)
    Include(Glob),
}

impl PatternType {
    /// Parse a pattern string into a PatternType
    /// Patterns starting with '!' are include-only patterns
    /// Regular patterns are exclude patterns
    fn from_string(pattern: &str) -> Result<Self> {
        if let Some(stripped) = pattern.strip_prefix('!') {
            if stripped.is_empty() {
                return Err(crate::RheoError::project_config(
                    "negated pattern '!' must have content after '!'".to_string(),
                ));
            }
            Glob::new(stripped).map(PatternType::Include).map_err(|e| {
                crate::RheoError::project_config(format!(
                    "invalid include pattern '{}': {}",
                    pattern, e
                ))
            })
        } else {
            Glob::new(pattern).map(PatternType::Exclude).map_err(|e| {
                crate::RheoError::project_config(format!(
                    "invalid exclude pattern '{}': {}",
                    pattern, e
                ))
            })
        }
    }
}

/// Filter patterns for include/exclude logic
///
/// Supports both exclude patterns and include-only patterns (negated with '!').
/// A file is included if:
/// 1. It doesn't match any exclude pattern, AND
/// 2. If include patterns exist, it matches at least one
#[derive(Debug)]
pub struct FilterPatterns {
    /// Files matching these patterns are excluded
    exclude_set: GlobSet,
    /// If non-empty, only files matching these patterns are included
    include_set: GlobSet,
}

impl FilterPatterns {
    /// Build FilterPatterns from a list of pattern strings
    ///
    /// Patterns starting with '!' are include-only patterns.
    /// Regular patterns are exclude patterns.
    ///
    /// # Examples
    ///
    /// Include only .typ files:
    /// ```
    /// # use rheo::FilterPatterns;
    /// let filter = FilterPatterns::from_patterns(&["!**/*.typ".to_string()])?;
    /// # Ok::<(), rheo::RheoError>(())
    /// ```
    ///
    /// Exclude temps:
    /// ```
    /// # use rheo::FilterPatterns;
    /// let filter = FilterPatterns::from_patterns(&["*.tmp".to_string()])?;
    /// # Ok::<(), rheo::RheoError>(())
    /// ```
    ///
    /// Mixed (include .typ and images, exclude temps):
    /// ```
    /// # use rheo::FilterPatterns;
    /// let filter = FilterPatterns::from_patterns(&[
    ///     "!**/*.typ".to_string(),
    ///     "!img/**".to_string(),
    ///     "*.tmp".to_string(),
    /// ])?;
    /// # Ok::<(), rheo::RheoError>(())
    /// ```
    pub fn from_patterns(patterns: &[String]) -> Result<Self> {
        let mut exclude_builder = GlobSetBuilder::new();
        let mut include_builder = GlobSetBuilder::new();

        for pattern in patterns {
            match PatternType::from_string(pattern)? {
                PatternType::Exclude(glob) => {
                    exclude_builder.add(glob);
                    debug!(pattern = %pattern, "added exclude pattern");
                }
                PatternType::Include(glob) => {
                    include_builder.add(glob);
                    debug!(pattern = %pattern, "added include pattern");
                }
            }
        }

        let exclude_set = exclude_builder.build().map_err(|e| {
            crate::RheoError::project_config(format!("failed to build exclude patterns: {}", e))
        })?;

        let include_set = include_builder.build().map_err(|e| {
            crate::RheoError::project_config(format!("failed to build include patterns: {}", e))
        })?;

        Ok(FilterPatterns {
            exclude_set,
            include_set,
        })
    }

    /// Check if a file should be included based on the filter patterns
    ///
    /// Logic:
    /// - If file matches any exclude pattern, return false
    /// - If include patterns exist and file doesn't match any, return false
    /// - Otherwise return true
    pub fn should_include(&self, path: &Path) -> bool {
        // Excluded if matches any exclude pattern
        if self.exclude_set.is_match(path) {
            return false;
        }

        // If include patterns exist, must match at least one
        if !self.include_set.is_empty() {
            return self.include_set.is_match(path);
        }

        // No include patterns and not excluded = include
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_patterns_include_only_typ() {
        let filter = FilterPatterns::from_patterns(&["!**/*.typ".to_string()]).unwrap();
        assert!(filter.should_include(Path::new("doc.typ")));
        assert!(filter.should_include(Path::new("subdir/article.typ")));
        assert!(!filter.should_include(Path::new("image.png")));
        assert!(!filter.should_include(Path::new("data.json")));
    }

    #[test]
    fn test_filter_patterns_include_only_images() {
        let filter = FilterPatterns::from_patterns(&["!img/**".to_string()]).unwrap();
        assert!(filter.should_include(Path::new("img/photo.jpg")));
        assert!(filter.should_include(Path::new("img/icons/star.svg")));
        assert!(!filter.should_include(Path::new("doc.typ")));
        assert!(!filter.should_include(Path::new("data.json")));
    }

    #[test]
    fn test_filter_patterns_exclude_only_temps() {
        let filter = FilterPatterns::from_patterns(&["*.tmp".to_string()]).unwrap();
        assert!(!filter.should_include(Path::new("file.tmp")));
        assert!(filter.should_include(Path::new("file.txt")));
        assert!(filter.should_include(Path::new("doc.typ")));
    }

    #[test]
    fn test_filter_patterns_exclude_directory() {
        let filter = FilterPatterns::from_patterns(&["_drafts/**".to_string()]).unwrap();
        assert!(!filter.should_include(Path::new("_drafts/article.typ")));
        assert!(!filter.should_include(Path::new("_drafts/subdir/notes.typ")));
        assert!(filter.should_include(Path::new("published/article.typ")));
    }

    #[test]
    fn test_filter_patterns_mixed_include_typ_and_images() {
        let filter =
            FilterPatterns::from_patterns(&["!**/*.typ".to_string(), "!img/**".to_string()])
                .unwrap();
        assert!(filter.should_include(Path::new("doc.typ")));
        assert!(filter.should_include(Path::new("img/photo.jpg")));
        assert!(!filter.should_include(Path::new("data.json")));
        assert!(!filter.should_include(Path::new("script.js")));
    }

    #[test]
    fn test_filter_patterns_mixed_include_with_exclude() {
        let filter =
            FilterPatterns::from_patterns(&["!img/**".to_string(), "*.tmp".to_string()]).unwrap();
        // Include img files
        assert!(filter.should_include(Path::new("img/photo.jpg")));
        // But exclude .tmp files even in img/
        assert!(!filter.should_include(Path::new("img/temp.tmp")));
        // Non-img files not included (because of include filter)
        assert!(!filter.should_include(Path::new("doc.typ")));
    }

    #[test]
    fn test_filter_patterns_multiple_includes_or_logic() {
        let filter = FilterPatterns::from_patterns(&[
            "!**/*.typ".to_string(),
            "!img/**".to_string(),
            "!css/**".to_string(),
        ])
        .unwrap();
        // All these should be included
        assert!(filter.should_include(Path::new("doc.typ")));
        assert!(filter.should_include(Path::new("img/photo.jpg")));
        assert!(filter.should_include(Path::new("css/style.css")));
        // But not this
        assert!(!filter.should_include(Path::new("data.json")));
    }

    #[test]
    fn test_filter_patterns_empty_defaults_to_include_all() {
        let filter = FilterPatterns::from_patterns(&[]).unwrap();
        assert!(filter.should_include(Path::new("anything.txt")));
        assert!(filter.should_include(Path::new("doc.typ")));
        assert!(filter.should_include(Path::new("image.png")));
    }

    #[test]
    fn test_filter_patterns_exclude_takes_precedence() {
        let filter = FilterPatterns::from_patterns(&[
            "!**/*.typ".to_string(),
            "**/*.typ".to_string(), // Exclude all .typ
        ])
        .unwrap();
        // Exclude wins over include
        assert!(!filter.should_include(Path::new("doc.typ")));
    }

    #[test]
    fn test_filter_patterns_invalid_glob() {
        let result = FilterPatterns::from_patterns(&["[invalid".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_filter_patterns_empty_negated_pattern() {
        let result = FilterPatterns::from_patterns(&["!".to_string()]);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("must have content after '!'"));
    }

    #[test]
    fn test_filter_patterns_complex_scenario() {
        // Include only .typ and img, but exclude drafts and temps
        let filter = FilterPatterns::from_patterns(&[
            "!**/*.typ".to_string(),
            "!img/**".to_string(),
            "_drafts/**".to_string(),
            "*.tmp".to_string(),
        ])
        .unwrap();

        // .typ files included
        assert!(filter.should_include(Path::new("article.typ")));
        // Images included
        assert!(filter.should_include(Path::new("img/photo.jpg")));
        // But not drafts
        assert!(!filter.should_include(Path::new("_drafts/article.typ")));
        // And not temps
        assert!(!filter.should_include(Path::new("temp.tmp")));
        // And not other files
        assert!(!filter.should_include(Path::new("data.json")));
    }
}

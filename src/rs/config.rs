use crate::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

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
            Glob::new(stripped)
                .map(PatternType::Include)
                .map_err(|e| {
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
    /// let filter = FilterPatterns::from_patterns(&["!**/*.typ".to_string()])?;
    /// ```
    ///
    /// Exclude temps:
    /// ```
    /// let filter = FilterPatterns::from_patterns(&["*.tmp".to_string()])?;
    /// ```
    ///
    /// Mixed (include .typ and images, exclude temps):
    /// ```
    /// let filter = FilterPatterns::from_patterns(&[
    ///     "!**/*.typ".to_string(),
    ///     "!img/**".to_string(),
    ///     "*.tmp".to_string(),
    /// ])?;
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
        if self.include_set.len() > 0 {
            return self.include_set.is_match(path);
        }

        // No include patterns and not excluded = include
        true
    }
}

/// Configuration for rheo compilation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RheoConfig {
    /// Directory containing .typ content files (relative to project root)
    /// If not specified, searches entire project root
    /// Example: "content"
    pub content_dir: Option<String>,

    #[serde(default)]
    pub compile: CompileConfig,

    /// HTML-specific configuration
    #[serde(default)]
    pub html: HtmlConfig,

    /// PDF-specific configuration
    #[serde(default)]
    pub pdf: PdfConfig,

    /// EPUB-specific configuration
    #[serde(default)]
    pub epub: EpubConfig,
}

/// Compilation-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileConfig {
    /// Glob patterns for files to exclude from compilation
    /// Patterns are evaluated relative to content_dir (or project root if content_dir not set)
    /// Example: ["lib/**/*.typ", "_*/**"]
    #[serde(default = "default_exclude_patterns")]
    pub exclude: Vec<String>,

    /// Default formats to compile (if none specified via CLI)
    /// Example: ["pdf", "html"]
    #[serde(default = "default_formats")]
    pub formats: Vec<String>,
}

/// HTML output configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HtmlConfig {
    /// Glob patterns for static files to copy to HTML output
    /// Patterns are evaluated relative to content_dir (or project root if content_dir not set)
    /// Example: ["img/**", "css/**", "data/*.json"]
    #[serde(default)]
    pub static_files: Vec<String>,

    /// Glob patterns for files to exclude from HTML compilation
    /// Patterns are evaluated relative to content_dir (or project root if content_dir not set)
    /// Example: ["index.typ", "pdf-only/**"]
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// PDF output configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PdfConfig {
    /// Glob patterns for files to exclude from PDF compilation
    /// Patterns are evaluated relative to content_dir (or project root if content_dir not set)
    /// Example: ["index.typ", "web-only/**"]
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// EPUB output configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EpubConfig {
    /// Glob patterns for files to exclude from EPUB compilation
    /// Patterns are evaluated relative to content_dir (or project root if content_dir not set)
    /// Example: ["index.typ", "web-only/**"]
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl Default for CompileConfig {
    fn default() -> Self {
        Self {
            exclude: default_exclude_patterns(),
            formats: default_formats(),
        }
    }
}

/// Default exclusion patterns
fn default_exclude_patterns() -> Vec<String> {
    vec!["lib/**/*.typ".to_string()]
}

/// Default output formats
fn default_formats() -> Vec<String> {
    vec!["pdf".to_string(), "html".to_string()]
}

impl RheoConfig {
    /// Load configuration from rheo.toml in the given directory
    /// If the file doesn't exist, returns default configuration
    pub fn load(project_root: &Path) -> Result<Self> {
        let config_path = project_root.join("rheo.toml");

        if !config_path.exists() {
            debug!(path = %config_path.display(), "no rheo.toml found, using defaults");
            return Ok(Self::default());
        }

        info!(path = %config_path.display(), "loading configuration");
        let contents = std::fs::read_to_string(&config_path)
            .map_err(|e| crate::RheoError::io(e, format!("reading {}", config_path.display())))?;

        let config: RheoConfig = toml::from_str(&contents)
            .map_err(|e| crate::RheoError::project_config(format!("invalid rheo.toml: {}", e)))?;

        debug!(exclude_patterns = ?config.compile.exclude, "loaded configuration");
        Ok(config)
    }

    /// Build a GlobSet from the exclusion patterns for efficient matching
    pub fn build_exclusion_set(&self) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();

        for pattern in &self.compile.exclude {
            match Glob::new(pattern) {
                Ok(glob) => {
                    builder.add(glob);
                    debug!(pattern = %pattern, "added exclusion pattern");
                }
                Err(e) => {
                    warn!(pattern = %pattern, error = %e, "invalid glob pattern, skipping");
                }
            }
        }

        builder.build().map_err(|e| {
            crate::RheoError::project_config(format!("failed to build exclusion set: {}", e))
        })
    }

    /// Resolve content_dir to an absolute path if configured
    /// Returns None if content_dir is not set or doesn't exist
    pub fn resolve_content_dir(&self, project_root: &Path) -> Option<std::path::PathBuf> {
        self.content_dir.as_ref().map(|dir| {
            let path = project_root.join(dir);
            debug!(content_dir = %path.display(), "resolved content directory");
            path
        })
    }

    /// Get static files glob patterns for HTML output
    /// Returns empty slice if not configured
    pub fn get_static_files_patterns(&self) -> &[String] {
        &self.html.static_files
    }

    /// Build GlobSet for HTML exclusions
    pub fn build_html_exclusion_set(&self) -> Result<GlobSet> {
        Self::build_globset(&self.html.exclude, "html.exclude")
    }

    /// Build GlobSet for PDF exclusions
    pub fn build_pdf_exclusion_set(&self) -> Result<GlobSet> {
        Self::build_globset(&self.pdf.exclude, "pdf.exclude")
    }

    /// Build GlobSet for EPUB exclusions
    pub fn build_epub_exclusion_set(&self) -> Result<GlobSet> {
        Self::build_globset(&self.epub.exclude, "epub.exclude")
    }

    /// Helper to build a GlobSet from validated patterns
    fn build_globset(patterns: &[String], name: &str) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();

        for pattern in patterns {
            // Patterns are already validated, so this should not fail
            let glob = Glob::new(pattern).map_err(|e| {
                crate::RheoError::project_config(format!(
                    "failed to build {} glob from validated pattern '{}': {}",
                    name, pattern, e
                ))
            })?;
            builder.add(glob);
            debug!(pattern = %pattern, filter = %name, "added format-specific pattern");
        }

        builder.build().map_err(|e| {
            crate::RheoError::project_config(format!("failed to build {} globset: {}", name, e))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RheoConfig::default();
        assert_eq!(config.compile.exclude, vec!["lib/**/*.typ"]);
        assert_eq!(config.compile.formats, vec!["pdf", "html"]);
    }

    #[test]
    fn test_exclusion_set() {
        let config = RheoConfig::default();
        let exclusions = config.build_exclusion_set().unwrap();

        // Should match lib files
        assert!(exclusions.is_match("lib/foo.typ"));
        assert!(exclusions.is_match("lib/subdir/bar.typ"));

        // Should not match non-lib files
        assert!(!exclusions.is_match("main.typ"));
        assert!(!exclusions.is_match("src/main.typ"));
    }
}

#[test]
fn test_per_format_exclusions_default_empty() {
    let config = RheoConfig::default();
    assert!(config.html.exclude.is_empty());
    assert!(config.pdf.exclude.is_empty());
    assert!(config.epub.exclude.is_empty());
}

#[test]
fn test_per_format_exclusion_patterns() {
    let toml = r#"
            [html]
            exclude = ["pdf-only/**/*.typ"]
            
            [pdf]
            exclude = ["index.typ", "web/**/*.typ"]
            
            [epub]
            exclude = ["index.typ"]
        "#;

    let config: RheoConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.html.exclude, vec!["pdf-only/**/*.typ"]);
    assert_eq!(config.pdf.exclude, vec!["index.typ", "web/**/*.typ"]);
    assert_eq!(config.epub.exclude, vec!["index.typ"]);
}

#[test]
fn test_build_per_format_exclusion_sets() {
    let toml = r#"
            [html]
            exclude = ["pdf-only/**/*.typ"]
            
            [pdf]
            exclude = ["web/**/*.typ"]
        "#;

    let config: RheoConfig = toml::from_str(toml).unwrap();
    let html_exclusions = config.build_html_exclusion_set().unwrap();
    let pdf_exclusions = config.build_pdf_exclusion_set().unwrap();

    // Test HTML exclusions
    assert!(html_exclusions.is_match("pdf-only/doc.typ"));
    assert!(!html_exclusions.is_match("web/index.typ"));

    // Test PDF exclusions
    assert!(pdf_exclusions.is_match("web/index.typ"));
    assert!(!pdf_exclusions.is_match("pdf-only/doc.typ"));
}

#[test]
fn test_formats_from_config() {
    let toml = r#"
            [compile]
            formats = ["pdf"]
        "#;

    let config: RheoConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.compile.formats, vec!["pdf"]);
}

#[test]
fn test_formats_defaults_when_not_specified() {
    let toml = r#"
            [compile]
            exclude = ["lib/**/*.typ"]
        "#;

    let config: RheoConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.compile.formats, vec!["pdf", "html"]);
}

#[test]
fn test_formats_multiple_values() {
    let toml = r#"
            [compile]
            formats = ["html", "epub"]
        "#;

    let config: RheoConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.compile.formats, vec!["html", "epub"]);
}

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
    let filter = FilterPatterns::from_patterns(&[
        "!**/*.typ".to_string(),
        "!img/**".to_string(),
    ])
    .unwrap();
    assert!(filter.should_include(Path::new("doc.typ")));
    assert!(filter.should_include(Path::new("img/photo.jpg")));
    assert!(!filter.should_include(Path::new("data.json")));
    assert!(!filter.should_include(Path::new("script.js")));
}

#[test]
fn test_filter_patterns_mixed_include_with_exclude() {
    let filter = FilterPatterns::from_patterns(&[
        "!img/**".to_string(),
        "*.tmp".to_string(),
    ])
    .unwrap();
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

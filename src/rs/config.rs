use crate::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

/// Configuration for rheo compilation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RheoConfig {
    #[serde(default)]
    pub compile: CompileConfig,

    /// HTML-specific configuration
    #[serde(default)]
    pub html: HtmlConfig,
}

/// Compilation-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileConfig {
    /// Glob patterns for files to exclude from compilation
    /// Example: ["lib/**/*.typ", "_*/**"]
    #[serde(default = "default_exclude_patterns")]
    pub exclude: Vec<String>,

    /// Directory containing .typ content files (relative to project root)
    /// If not specified, searches entire project root
    /// Example: "content"
    pub content_dir: Option<String>,

    /// Glob patterns for files that should only be compiled to HTML
    /// Example: ["web/**/*.typ", "index.typ"]
    #[serde(default)]
    #[serde(deserialize_with = "validate_glob_patterns")]
    pub html_only: Vec<String>,

    /// Glob patterns for files that should only be compiled to PDF
    /// Example: ["print/**/*.typ"]
    #[serde(default)]
    #[serde(deserialize_with = "validate_glob_patterns")]
    pub pdf_only: Vec<String>,

    /// Glob patterns for files that should only be compiled to EPUB
    /// Example: ["ebook/**/*.typ"]
    #[serde(default)]
    #[serde(deserialize_with = "validate_glob_patterns")]
    pub epub_only: Vec<String>,
}

/// HTML output configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HtmlConfig {
    /// Glob patterns for static files to copy to HTML output
    /// Patterns are evaluated relative to project root
    /// Example: ["img/**", "css/**", "data/*.json"]
    #[serde(default)]
    pub static_files: Vec<String>,
}

impl Default for CompileConfig {
    fn default() -> Self {
        Self {
            exclude: default_exclude_patterns(),
            content_dir: None,
            html_only: Vec::new(),
            pdf_only: Vec::new(),
            epub_only: Vec::new(),
        }
    }
}

/// Default exclusion patterns
fn default_exclude_patterns() -> Vec<String> {
    vec!["lib/**/*.typ".to_string()]
}

/// Strict validation for glob patterns - fails deserialization if any pattern is invalid
fn validate_glob_patterns<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let patterns: Vec<String> = Vec::deserialize(deserializer)?;

    // Validate each pattern
    for pattern in &patterns {
        Glob::new(pattern).map_err(|e| {
            D::Error::custom(format!("invalid glob pattern '{}': {}", pattern, e))
        })?;
    }

    Ok(patterns)
}

/// Holds compiled GlobSets for each format-specific filter
#[derive(Debug)]
pub struct FormatFilterSets {
    pub html_only: GlobSet,
    pub pdf_only: GlobSet,
    pub epub_only: GlobSet,
}

impl Default for RheoConfig {
    fn default() -> Self {
        Self {
            compile: CompileConfig::default(),
            html: HtmlConfig::default(),
        }
    }
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

        builder.build()
            .map_err(|e| crate::RheoError::project_config(format!("failed to build exclusion set: {}", e)))
    }

    /// Resolve content_dir to an absolute path if configured
    /// Returns None if content_dir is not set or doesn't exist
    pub fn resolve_content_dir(&self, project_root: &Path) -> Option<std::path::PathBuf> {
        self.compile.content_dir.as_ref().map(|dir| {
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

    /// Build GlobSets for format-specific file patterns
    /// Note: Patterns were already validated during deserialization
    pub fn build_format_filter_sets(&self) -> Result<FormatFilterSets> {
        let html_only = Self::build_globset(&self.compile.html_only, "html_only")?;
        let pdf_only = Self::build_globset(&self.compile.pdf_only, "pdf_only")?;
        let epub_only = Self::build_globset(&self.compile.epub_only, "epub_only")?;

        Ok(FormatFilterSets {
            html_only,
            pdf_only,
            epub_only,
        })
    }

    /// Helper to build a GlobSet from validated patterns
    fn build_globset(patterns: &[String], name: &str) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();

        for pattern in patterns {
            // Patterns are already validated, so this should not fail
            let glob = Glob::new(pattern)
                .map_err(|e| crate::RheoError::project_config(
                    format!("failed to build {} glob from validated pattern '{}': {}", name, pattern, e)
                ))?;
            builder.add(glob);
            debug!(pattern = %pattern, filter = %name, "added format-specific pattern");
        }

        builder.build()
            .map_err(|e| crate::RheoError::project_config(
                format!("failed to build {} globset: {}", name, e)
            ))
    }

    /// Check if a file matches multiple format-specific patterns (which is an error)
    /// Returns an error if the file matches more than one format filter
    pub fn check_format_conflicts(
        &self,
        file_path: &Path,
        filter_sets: &FormatFilterSets,
        project_root: &Path,
    ) -> Result<()> {
        // Make path relative to project root for matching
        let relative_path = file_path.strip_prefix(project_root)
            .map_err(|_| crate::RheoError::path(
                file_path,
                format!("file is not within project root {}", project_root.display())
            ))?;

        let mut matched_formats = Vec::new();

        if filter_sets.html_only.is_match(relative_path) {
            matched_formats.push("html_only");
        }
        if filter_sets.pdf_only.is_match(relative_path) {
            matched_formats.push("pdf_only");
        }
        if filter_sets.epub_only.is_match(relative_path) {
            matched_formats.push("epub_only");
        }

        if matched_formats.len() > 1 {
            return Err(crate::RheoError::project_config(format!(
                "file '{}' matches multiple format-specific patterns: {}",
                relative_path.display(),
                matched_formats.join(", ")
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RheoConfig::default();
        assert_eq!(config.compile.exclude, vec!["lib/**/*.typ"]);
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

    #[test]
    fn test_format_specific_patterns_default_empty() {
        let config = RheoConfig::default();
        assert!(config.compile.html_only.is_empty());
        assert!(config.compile.pdf_only.is_empty());
        assert!(config.compile.epub_only.is_empty());
    }

    #[test]
    fn test_valid_format_patterns() {
        let toml = r#"
            [compile]
            html_only = ["web/**/*.typ", "index.typ"]
            pdf_only = ["print/**/*.typ"]
            epub_only = ["ebook/**/*.typ"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.compile.html_only, vec!["web/**/*.typ", "index.typ"]);
        assert_eq!(config.compile.pdf_only, vec!["print/**/*.typ"]);
        assert_eq!(config.compile.epub_only, vec!["ebook/**/*.typ"]);
    }

    #[test]
    fn test_invalid_glob_pattern_fails() {
        let toml = r#"
            [compile]
            html_only = ["web/[invalid.typ"]
        "#;

        let result: std::result::Result<RheoConfig, _> = toml::from_str(toml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid glob pattern"));
        assert!(err_msg.contains("web/[invalid.typ"));
    }

    #[test]
    fn test_mixed_valid_and_invalid_pattern_fails() {
        let toml = r#"
            [compile]
            html_only = ["valid/**/*.typ", "invalid/[*.typ"]
        "#;

        let result: std::result::Result<RheoConfig, _> = toml::from_str(toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_format_filter_sets() {
        let toml = r#"
            [compile]
            html_only = ["web/**/*.typ"]
            pdf_only = ["print/**/*.typ"]
            epub_only = ["ebook/**/*.typ"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let filter_sets = config.build_format_filter_sets().unwrap();

        // Test HTML only patterns
        assert!(filter_sets.html_only.is_match("web/index.typ"));
        assert!(filter_sets.html_only.is_match("web/sub/page.typ"));
        assert!(!filter_sets.html_only.is_match("print/doc.typ"));

        // Test PDF only patterns
        assert!(filter_sets.pdf_only.is_match("print/doc.typ"));
        assert!(!filter_sets.pdf_only.is_match("web/index.typ"));

        // Test EPUB only patterns
        assert!(filter_sets.epub_only.is_match("ebook/chapter.typ"));
        assert!(!filter_sets.epub_only.is_match("web/index.typ"));
    }

    #[test]
    fn test_no_conflict_when_different_files() {
        let toml = r#"
            [compile]
            html_only = ["web/**/*.typ"]
            pdf_only = ["print/**/*.typ"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let filter_sets = config.build_format_filter_sets().unwrap();
        let project_root = std::path::PathBuf::from("/tmp/project");

        // Different files - no conflict
        let web_file = project_root.join("web/index.typ");
        assert!(config.check_format_conflicts(&web_file, &filter_sets, &project_root).is_ok());

        let print_file = project_root.join("print/doc.typ");
        assert!(config.check_format_conflicts(&print_file, &filter_sets, &project_root).is_ok());
    }

    #[test]
    fn test_conflict_detected_html_and_pdf() {
        let toml = r#"
            [compile]
            html_only = ["**/*.typ"]
            pdf_only = ["docs/**/*.typ"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let filter_sets = config.build_format_filter_sets().unwrap();
        let project_root = std::path::PathBuf::from("/tmp/project");

        // File matches both patterns
        let conflicting_file = project_root.join("docs/guide.typ");
        let result = config.check_format_conflicts(&conflicting_file, &filter_sets, &project_root);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("matches multiple format-specific patterns"));
        assert!(err_msg.contains("html_only"));
        assert!(err_msg.contains("pdf_only"));
    }

    #[test]
    fn test_conflict_detected_all_three_formats() {
        let toml = r#"
            [compile]
            html_only = ["**/*.typ"]
            pdf_only = ["**/*.typ"]
            epub_only = ["**/*.typ"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let filter_sets = config.build_format_filter_sets().unwrap();
        let project_root = std::path::PathBuf::from("/tmp/project");

        let file = project_root.join("any.typ");
        let result = config.check_format_conflicts(&file, &filter_sets, &project_root);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("html_only"));
        assert!(err_msg.contains("pdf_only"));
        assert!(err_msg.contains("epub_only"));
    }

    #[test]
    fn test_no_conflict_when_no_patterns_match() {
        let toml = r#"
            [compile]
            html_only = ["web/**/*.typ"]
            pdf_only = ["print/**/*.typ"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let filter_sets = config.build_format_filter_sets().unwrap();
        let project_root = std::path::PathBuf::from("/tmp/project");

        // File doesn't match any pattern - no conflict
        let other_file = project_root.join("other/file.typ");
        assert!(config.check_format_conflicts(&other_file, &filter_sets, &project_root).is_ok());
    }

    #[test]
    fn test_empty_filter_sets() {
        let config = RheoConfig::default();
        let filter_sets = config.build_format_filter_sets().unwrap();
        let project_root = std::path::PathBuf::from("/tmp/project");

        // No patterns configured - should never conflict
        let file = project_root.join("any.typ");
        assert!(config.check_format_conflicts(&file, &filter_sets, &project_root).is_ok());
    }
}

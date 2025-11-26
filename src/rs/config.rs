use crate::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

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
        }
    }
}

/// Default exclusion patterns
fn default_exclude_patterns() -> Vec<String> {
    vec!["lib/**/*.typ".to_string()]
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

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
        }
    }
}

/// Default exclusion patterns
fn default_exclude_patterns() -> Vec<String> {
    vec!["lib/**/*.typ".to_string()]
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

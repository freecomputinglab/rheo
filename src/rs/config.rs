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
}

/// Compilation-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileConfig {
    /// Glob patterns for files to exclude from compilation
    /// Example: ["lib/**/*.typ", "_*/**"]
    #[serde(default = "default_exclude_patterns")]
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

impl Default for RheoConfig {
    fn default() -> Self {
        Self {
            compile: CompileConfig::default(),
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

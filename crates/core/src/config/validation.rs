use crate::config::Spine;
use crate::config::manifest_version::ManifestVersion;
use crate::{Result, RheoConfig, RheoError};
use tracing::warn;

/// Trait for validating configuration structs after deserialization.
pub trait ValidateConfig {
    fn validate(&self) -> Result<()>;
}

impl ValidateConfig for RheoConfig {
    fn validate(&self) -> Result<()> {
        // Check version match
        let current = ManifestVersion::current();
        if self.version != current {
            warn!(
                "rheo.toml version {} does not match rheo version {}. \
                 Consider updating your rheo.toml version field.",
                self.version, current
            );
        }

        // Validate every plugin section's spine
        for (name, section) in &self.plugin_sections {
            if let Some(spine) = &section.spine {
                spine
                    .validate()
                    .map_err(|e| RheoError::project_config(format!("[{}]: {}", name, e)))?;
            }
        }

        Ok(())
    }
}

/// Validate glob patterns in a vertebrae list.
fn validate_vertebrae(vertebrae: &[String]) -> Result<()> {
    for pattern in vertebrae {
        glob::Pattern::new(pattern).map_err(|e| {
            RheoError::project_config(format!("invalid glob pattern '{}': {}", pattern, e))
        })?;
    }
    Ok(())
}

impl ValidateConfig for Spine {
    fn validate(&self) -> Result<()> {
        validate_vertebrae(&self.vertebrae)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universal_spine_validate_empty() {
        let spine = Spine {
            title: Some("Test".to_string()),
            vertebrae: vec![],
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_universal_spine_validate_valid_patterns() {
        let spine = Spine {
            title: Some("Test".to_string()),
            vertebrae: vec!["*.typ".to_string(), "chapters/**/*.typ".to_string()],
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_universal_spine_validate_invalid_pattern() {
        let spine = Spine {
            title: Some("Test".to_string()),
            vertebrae: vec!["[invalid".to_string()],
        };
        let result = spine.validate();
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("invalid glob pattern"));
    }

    #[test]
    fn test_rheo_config_validates_with_matching_version() {
        let toml = format!("version = \"{}\"", env!("CARGO_PKG_VERSION"));
        let raw: crate::config::RheoConfigRaw = toml::from_str(&toml).unwrap();
        let config = RheoConfig::try_from(raw).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rheo_config_warns_on_newer_version() {
        let toml = r#"version = "99.0.0""#;
        let raw: crate::config::RheoConfigRaw = toml::from_str(toml).unwrap();
        let config = RheoConfig::try_from(raw).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rheo_config_warns_on_older_version() {
        let toml = r#"version = "0.0.1""#;
        let raw: crate::config::RheoConfigRaw = toml::from_str(toml).unwrap();
        let config = RheoConfig::try_from(raw).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rheo_config_validates_plugin_sections() {
        let toml = format!(
            "version = \"{}\"\n[pdf.spine]\ntitle = \"Book\"\nvertebrae = [\"*.typ\"]\nmerge = true",
            env!("CARGO_PKG_VERSION")
        );
        let raw: crate::config::RheoConfigRaw = toml::from_str(&toml).unwrap();
        let config = RheoConfig::try_from(raw).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rheo_config_rejects_invalid_glob_in_section() {
        let toml = format!(
            "version = \"{}\"\n[pdf.spine]\nvertebrae = [\"[invalid\"]",
            env!("CARGO_PKG_VERSION")
        );
        let raw: crate::config::RheoConfigRaw = toml::from_str(&toml).unwrap();
        let config = RheoConfig::try_from(raw).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid glob pattern")
        );
    }
}

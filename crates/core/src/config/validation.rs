use crate::config::Spine;
use crate::config::manifest_version::ManifestVersion;
use crate::config::retired::warn_on_retired_keys;
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
                "rheo.toml version {} does not match rheo version {}. Run `rheo migrate` to \
                 update it (add --apply to write the changes).",
                self.version, current
            );
        }

        // Validate the global spine, then every plugin section's own (its
        // extra keys, plus any nested `[<plugin>.spine]`).
        if let Some(spine) = &self.spine {
            spine
                .validate()
                .map_err(|e| RheoError::project_config(format!("[spine]: {}", e)))?;
        }
        for (name, section) in &self.plugin_sections {
            warn_on_retired_keys(&format!("[{}]", name), &section.extra);
            if let Some(spine) = &section.spine {
                spine
                    .validate()
                    .map_err(|e| RheoError::project_config(format!("[{}]: {}", name, e)))?;
            }
        }

        Ok(())
    }
}

impl ValidateConfig for Spine {
    fn validate(&self) -> Result<()> {
        warn_on_retired_keys("[spine]", &self.extra);

        if self.include.is_some() && self.section.is_some() {
            return Err(RheoError::project_config(
                "include is a flat reorder, section nests into virtual directories: set one, not both",
            ));
        }

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
            ..Default::default()
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_universal_spine_validate_ignores_now_inert_vertebrae() {
        // `vertebrae` no longer affects the build, so its presence (even a
        // glob-invalid entry) is just a (untested-here) warning, not a
        // validation error.
        let mut extra = toml::Table::new();
        extra.insert(
            "vertebrae".to_string(),
            toml::Value::Array(vec![toml::Value::String("[invalid".to_string())]),
        );
        let spine = Spine {
            title: Some("Test".to_string()),
            extra,
            ..Default::default()
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_universal_spine_validate_ignores_retired_merge() {
        let mut extra = toml::Table::new();
        extra.insert("merge".to_string(), toml::Value::Boolean(true));
        let spine = Spine {
            title: Some("Test".to_string()),
            extra,
            ..Default::default()
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_universal_spine_validate_rejects_include_with_section() {
        let spine = Spine {
            include: Some(vec!["a.typ".to_string()]),
            section: Some(vec![]),
            ..Default::default()
        };
        let err = spine.validate().unwrap_err();
        assert!(err.to_string().contains("flat reorder"));
    }

    #[test]
    fn test_universal_spine_validate_ignores_unrecognized_key() {
        // A key not in RETIRED_KEYS (third-party or forward-compatible) must
        // never warn — `extra` is also how those survive.
        let mut extra = toml::Table::new();
        extra.insert(
            "some_future_key".to_string(),
            toml::Value::String("x".to_string()),
        );
        let spine = Spine {
            title: Some("Test".to_string()),
            extra,
            ..Default::default()
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_rheo_config_validates_global_spine() {
        // The global `[spine]` table (not nested under any `[<plugin>.spine]`)
        // must also be validated — a retired key set only there was
        // previously never checked at all.
        let toml = format!(
            "version = \"{}\"\n[spine]\nvertebrae = [\"*.typ\"]",
            env!("CARGO_PKG_VERSION")
        );
        let raw: crate::config::RheoConfigRaw = toml::from_str(&toml).unwrap();
        let config = RheoConfig::try_from(raw).unwrap();
        assert!(config.validate().is_ok());
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
    fn test_rheo_config_ignores_now_inert_vertebrae_in_section() {
        let toml = format!(
            "version = \"{}\"\n[pdf.spine]\nvertebrae = [\"[invalid\"]",
            env!("CARGO_PKG_VERSION")
        );
        let raw: crate::config::RheoConfigRaw = toml::from_str(&toml).unwrap();
        let config = RheoConfig::try_from(raw).unwrap();
        assert!(config.validate().is_ok());
    }
}

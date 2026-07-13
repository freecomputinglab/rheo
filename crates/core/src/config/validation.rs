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

/// Warn when a `rheo.toml` still sets the retired `vertebrae` glob list.
///
/// Since the structured-spine directory scan landed, `vertebrae` has no effect
/// on spine membership or order — that now comes from the directory scan plus
/// `[spine] exclude` / `[[spine.section]]`. Unlike a hard error, this stays a
/// warning: an inert key doesn't corrupt the build, but silently ignoring it
/// entirely risks a project's spine membership changing without the author
/// noticing (see https://rheo.ohrg.org/spines for the current model).
///
/// This is a one-off, named after the single retired field we currently have.
/// If a second retired-key warning is ever needed, generalize both of these
/// into a `RetiredKey` trait (key name + replacement message) rather than
/// adding another standalone function like this one.
fn warn_on_vertebrae(extra: &toml::Table) {
    if extra.contains_key("vertebrae") {
        warn!(
            "`vertebrae` no longer has any effect as of rheo 0.5.0 — spine membership and \
             order now come from a directory scan plus `[spine] exclude` / `[[spine.section]]`. \
             See https://rheo.ohrg.org/spines for the current model."
        );
    }
}

impl ValidateConfig for Spine {
    fn validate(&self) -> Result<()> {
        warn_on_vertebrae(&self.extra);

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

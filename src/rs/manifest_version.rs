use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Current supported manifest version (derived from Cargo.toml)
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Newtype wrapper around semver::Version for type-safe manifest versioning
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestVersion(semver::Version);

impl ManifestVersion {
    /// Returns the current supported manifest version
    pub fn current() -> Self {
        Self(semver::Version::parse(CURRENT).expect("CURRENT constant must be valid semver"))
    }

}

impl fmt::Display for ManifestVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for ManifestVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for ManifestVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match semver::Version::parse(&s) {
            Ok(version) => Ok(ManifestVersion(version)),
            Err(err) => Err(serde::de::Error::custom(format!(
                "invalid manifest version '{}': {}. Expected semver format (e.g., '0.1.0')",
                s, err
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_version_is_valid() {
        let version = ManifestVersion::current();
        // Verify it parses and matches the crate version
        assert_eq!(version.to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_matches_same_version() {
        let current = ManifestVersion::current();
        let same = ManifestVersion(semver::Version::parse(env!("CARGO_PKG_VERSION")).unwrap());
        assert_eq!(current, same);
    }

    #[test]
    fn test_mismatch_with_different_version() {
        let current = ManifestVersion::current();
        let different = ManifestVersion(semver::Version::parse("0.0.1").unwrap());
        assert_ne!(current, different);
    }

    #[test]
    fn test_deserialize_valid_version() {
        #[derive(serde::Deserialize)]
        struct Config {
            version: ManifestVersion,
        }

        let toml_str = format!("version = \"{}\"", env!("CARGO_PKG_VERSION"));
        let config: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.version, ManifestVersion::current());
    }

    #[test]
    fn test_deserialize_invalid_version() {
        #[derive(serde::Deserialize, Debug)]
        #[allow(dead_code)]
        struct Config {
            version: ManifestVersion,
        }

        let toml_str = r#"version = "not-a-version""#;
        let result: Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("invalid manifest version"));
        assert!(err_msg.contains("Expected semver format"));
    }

    #[test]
    fn test_display_format() {
        let version = ManifestVersion(semver::Version::parse("1.2.3").unwrap());
        assert_eq!(version.to_string(), "1.2.3");
    }

    #[test]
    fn test_deserialize_empty_string() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct Config {
            version: ManifestVersion,
        }

        let toml_str = r#"version = """#;
        let result: Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_whitespace() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct Config {
            version: ManifestVersion,
        }

        let toml_str = r#"version = " 0.1.0 ""#;
        let result: Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_git_tag_format() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct Config {
            version: ManifestVersion,
        }

        let toml_str = r#"version = "v0.1.0""#;
        let result: Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_partial_version() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct Config {
            version: ManifestVersion,
        }

        let toml_str = r#"version = "0.1""#;
        let result: Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }
}

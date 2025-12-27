use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// Current supported manifest version
pub const CURRENT: &str = "0.1.0";

/// Newtype wrapper around semver::Version for type-safe manifest versioning
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestVersion(semver::Version);

impl ManifestVersion {
    /// Returns the current supported manifest version
    pub fn current() -> Self {
        Self(semver::Version::parse(CURRENT).expect("CURRENT constant must be valid semver"))
    }

    /// Checks if this version is compatible with the supported version
    ///
    /// Returns true if self <= supported (forward compatibility)
    pub fn is_compatible_with(&self, supported: &ManifestVersion) -> bool {
        self.0 <= supported.0
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
        assert_eq!(version.to_string(), CURRENT);
    }

    #[test]
    fn test_is_compatible_with_same_version() {
        let v1 = ManifestVersion(semver::Version::parse("0.1.0").unwrap());
        let v2 = ManifestVersion(semver::Version::parse("0.1.0").unwrap());
        assert!(v1.is_compatible_with(&v2));
    }

    #[test]
    fn test_is_compatible_with_older_version() {
        let older = ManifestVersion(semver::Version::parse("0.1.0").unwrap());
        let newer = ManifestVersion(semver::Version::parse("0.2.0").unwrap());
        assert!(older.is_compatible_with(&newer));
    }

    #[test]
    fn test_is_not_compatible_with_newer_version() {
        let newer = ManifestVersion(semver::Version::parse("0.2.0").unwrap());
        let older = ManifestVersion(semver::Version::parse("0.1.0").unwrap());
        assert!(!newer.is_compatible_with(&older));
    }

    #[test]
    fn test_deserialize_valid_version() {
        #[derive(serde::Deserialize)]
        struct Config {
            version: ManifestVersion,
        }

        let toml_str = r#"version = "0.1.0""#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.version,
            ManifestVersion(semver::Version::parse("0.1.0").unwrap())
        );
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
        let version = ManifestVersion(semver::Version::parse("0.1.0").unwrap());
        assert_eq!(version.to_string(), "0.1.0");
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

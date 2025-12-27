use serde::{Deserialize, Deserializer};
use std::fmt;

/// Current supported manifest version
pub const CURRENT: &str = "0.1.0";

/// Newtype wrapper around semver::Version for type-safe manifest versioning
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestVersion(semver::Version);

impl ManifestVersion {
    /// Returns the current supported manifest version
    pub fn current() -> Self {
        Self(
            semver::Version::parse(CURRENT)
                .expect("CURRENT constant must be valid semver"),
        )
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

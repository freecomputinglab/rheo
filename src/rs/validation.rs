use crate::config::{EpubConfig, EpubSpine, HtmlConfig, HtmlSpine, PdfConfig, PdfSpine};
use crate::manifest_version::ManifestVersion;
use crate::{Result, RheoConfig, RheoError};
use tracing::warn;

/// Trait for validating configuration structs after deserialization.
///
/// Implementations should check configuration invariants and return
/// descriptive errors for invalid configurations. This enables early
/// error detection before attempting compilation.
pub trait ValidateConfig {
    /// Validate this configuration.
    ///
    /// # Errors
    /// Returns `RheoError::ProjectConfig` if the configuration is invalid.
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

        // Delegate to existing validation
        self.pdf.validate()?;
        self.html.validate()?;
        self.epub.validate()?;

        Ok(())
    }
}

impl ValidateConfig for PdfConfig {
    fn validate(&self) -> Result<()> {
        if let Some(spine) = &self.spine {
            spine.validate()?;
        }
        Ok(())
    }
}

impl ValidateConfig for HtmlConfig {
    fn validate(&self) -> Result<()> {
        if let Some(spine) = &self.spine {
            spine.validate()?;
        }
        // Stylesheet and font paths are validated at usage time
        Ok(())
    }
}

impl ValidateConfig for EpubConfig {
    fn validate(&self) -> Result<()> {
        if let Some(spine) = &self.spine {
            spine.validate()?;
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

impl ValidateConfig for PdfSpine {
    fn validate(&self) -> Result<()> {
        validate_vertebrae(&self.vertebrae)?;

        // PDF spine with merge=true requires a title
        if self.merge == Some(true) && self.title.is_none() {
            return Err(RheoError::project_config(
                "pdf.spine.title is required when merge=true",
            ));
        }

        Ok(())
    }
}

impl ValidateConfig for EpubSpine {
    fn validate(&self) -> Result<()> {
        validate_vertebrae(&self.vertebrae)?;
        // EPUB always merges, title is optional (can be inferred)
        Ok(())
    }
}

impl ValidateConfig for HtmlSpine {
    fn validate(&self) -> Result<()> {
        validate_vertebrae(&self.vertebrae)?;
        // HTML never merges, title is optional
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_spine_validate_empty() {
        let spine = PdfSpine {
            title: Some("Test".to_string()),
            vertebrae: vec![],
            merge: None,
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_pdf_spine_validate_valid_patterns() {
        let spine = PdfSpine {
            title: Some("Test".to_string()),
            vertebrae: vec!["*.typ".to_string(), "chapters/**/*.typ".to_string()],
            merge: None,
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_pdf_spine_validate_invalid_pattern() {
        let spine = PdfSpine {
            title: Some("Test".to_string()),
            vertebrae: vec!["[invalid".to_string()], // Unclosed bracket is invalid glob
            merge: None,
        };
        let result = spine.validate();
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("invalid glob pattern"));
    }

    #[test]
    fn test_pdf_spine_merge_true_requires_title() {
        let spine = PdfSpine {
            title: None,
            vertebrae: vec!["*.typ".to_string()],
            merge: Some(true),
        };
        let result = spine.validate();
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("title is required when merge=true"));
    }

    #[test]
    fn test_pdf_spine_merge_true_with_title_ok() {
        let spine = PdfSpine {
            title: Some("My Book".to_string()),
            vertebrae: vec!["*.typ".to_string()],
            merge: Some(true),
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_pdf_spine_merge_false_no_title_ok() {
        let spine = PdfSpine {
            title: None,
            vertebrae: vec!["*.typ".to_string()],
            merge: Some(false),
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_pdf_config_validate_with_valid_spine() {
        let spine = PdfSpine {
            title: Some("Test".to_string()),
            vertebrae: vec!["*.typ".to_string()],
            merge: None,
        };
        let config = PdfConfig { spine: Some(spine) };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pdf_config_validate_with_invalid_spine() {
        let spine = PdfSpine {
            title: Some("Test".to_string()),
            vertebrae: vec!["[invalid".to_string()],
            merge: None,
        };
        let config = PdfConfig { spine: Some(spine) };
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_pdf_config_validate_no_spine() {
        let config = PdfConfig { spine: None };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_epub_spine_validate() {
        let spine = EpubSpine {
            title: Some("Test".to_string()),
            vertebrae: vec!["*.typ".to_string()],
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_epub_spine_validate_no_title_ok() {
        let spine = EpubSpine {
            title: None,
            vertebrae: vec!["*.typ".to_string()],
        };
        // EPUB title is optional (can be inferred)
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_epub_config_validate() {
        let spine = EpubSpine {
            title: Some("Test".to_string()),
            vertebrae: vec!["*.typ".to_string()],
        };
        let config = EpubConfig {
            identifier: None,
            date: None,
            spine: Some(spine),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_html_spine_validate() {
        let spine = HtmlSpine {
            title: Some("Test".to_string()),
            vertebrae: vec!["*.typ".to_string()],
        };
        assert!(spine.validate().is_ok());
    }

    #[test]
    fn test_html_config_validate() {
        let config = HtmlConfig {
            stylesheets: vec!["style.css".to_string()],
            fonts: vec![],
            spine: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rheo_config_validates_with_matching_version() {
        let toml = format!("version = \"{}\"", env!("CARGO_PKG_VERSION"));
        let config: RheoConfig = toml::from_str(&toml).unwrap();
        // Should validate successfully without warnings
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rheo_config_warns_on_newer_version() {
        // Create config with version 99.0.0 (newer than current)
        let toml = r#"
        version = "99.0.0"
        "#;
        let config: RheoConfig = toml::from_str(toml).unwrap();
        // Should validate successfully but log warning (mismatch)
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_rheo_config_warns_on_older_version() {
        // Create config with version 0.0.1 (older than current)
        let toml = r#"
        version = "0.0.1"
        "#;
        let config: RheoConfig = toml::from_str(toml).unwrap();
        // Should validate successfully but log warning (mismatch)
        assert!(config.validate().is_ok());
    }
}

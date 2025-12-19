use crate::config::{EpubConfig, HtmlConfig, Merge, PdfConfig};
use crate::{Result, RheoError};

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

impl ValidateConfig for PdfConfig {
    fn validate(&self) -> Result<()> {
        if let Some(merge) = &self.merge {
            merge.validate()?;
        }
        Ok(())
    }
}

impl ValidateConfig for HtmlConfig {
    fn validate(&self) -> Result<()> {
        // No validation needed for HTML config currently
        // Stylesheet and font paths are validated at usage time
        Ok(())
    }
}

impl ValidateConfig for EpubConfig {
    fn validate(&self) -> Result<()> {
        if let Some(merge) = &self.merge {
            merge.validate()?;
        }
        Ok(())
    }
}

impl ValidateConfig for Merge {
    fn validate(&self) -> Result<()> {
        // Empty spine is allowed - it has special behavior for single-file mode
        // See spine.rs lines 62-87

        // Validate that all glob patterns are syntactically valid
        for pattern in &self.spine {
            glob::Pattern::new(pattern).map_err(|e| {
                RheoError::project_config(format!("invalid glob pattern '{}': {}", pattern, e))
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_validate_empty_spine() {
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec![],
        };
        assert!(merge.validate().is_ok());
    }

    #[test]
    fn test_merge_validate_valid_patterns() {
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec!["*.typ".to_string(), "chapters/**/*.typ".to_string()],
        };
        assert!(merge.validate().is_ok());
    }

    #[test]
    fn test_merge_validate_invalid_pattern() {
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec!["[invalid".to_string()], // Unclosed bracket is invalid glob
        };
        let result = merge.validate();
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("invalid glob pattern"));
    }

    #[test]
    fn test_pdf_config_validate_with_valid_merge() {
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec!["*.typ".to_string()],
        };
        let config = PdfConfig { merge: Some(merge) };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_pdf_config_validate_with_invalid_merge() {
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec!["[invalid".to_string()],
        };
        let config = PdfConfig { merge: Some(merge) };
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_pdf_config_validate_no_merge() {
        let config = PdfConfig { merge: None };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_epub_config_validate() {
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec!["*.typ".to_string()],
        };
        let config = EpubConfig {
            identifier: None,
            date: None,
            merge: Some(merge),
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_html_config_validate() {
        let config = HtmlConfig {
            stylesheets: vec!["style.css".to_string()],
            fonts: vec![],
        };
        assert!(config.validate().is_ok());
    }
}

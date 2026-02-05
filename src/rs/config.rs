use crate::manifest_version::ManifestVersion;
use crate::validation::ValidateConfig;
use crate::{OutputFormat, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::debug;

/// HTML compilation options.
///
/// Controls HTML-specific behavior like stylesheet and font injection.
#[derive(Debug, Clone)]
pub struct HtmlOptions {
    /// Stylesheet paths to inject (relative to build dir)
    pub stylesheets: Vec<String>,
    /// Font URLs to inject
    pub fonts: Vec<String>,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        Self {
            stylesheets: default_stylesheets(),
            fonts: default_fonts(),
        }
    }
}

fn default_stylesheets() -> Vec<String> {
    vec!["style.css".to_string()]
}

fn default_fonts() -> Vec<String> {
    vec![]
}

/// EPUB compilation options.
///
/// Wraps the EpubConfig for use in the unified compilation interface.
#[derive(Debug, Clone)]
pub struct EpubOptions {
    /// Reference to the EPUB configuration
    pub config: EpubConfig,
}

impl From<&EpubConfig> for EpubOptions {
    fn from(config: &EpubConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

fn default_formats() -> Vec<OutputFormat> {
    OutputFormat::all_variants()
}

/// Configuration for rheo compilation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RheoConfig {
    /// Manifest version for API compatibility (required)
    pub version: ManifestVersion,

    /// Directory containing .typ content files (relative to project root)
    /// If not specified, searches entire project root
    /// Example: "content"
    pub content_dir: Option<String>,

    /// Build output directory (relative to project root unless absolute)
    /// Defaults to "build/" if not specified
    /// Examples: "output", "../shared-build", "/tmp/rheo-build"
    pub build_dir: Option<String>,

    /// Default formats to compile (if none specified via CLI).
    /// Example: ["pdf", "html", "epub"]
    #[serde(default = "default_formats")]
    pub formats: Vec<OutputFormat>,

    /// HTML-specific configuration
    #[serde(default)]
    pub html: HtmlConfig,

    /// PDF-specific configuration
    #[serde(default)]
    pub pdf: PdfConfig,

    /// EPUB-specific configuration
    #[serde(default)]
    pub epub: EpubConfig,
}

impl Default for RheoConfig {
    fn default() -> Self {
        Self {
            version: ManifestVersion::current(),
            content_dir: Some("./".to_string()),
            build_dir: Some("./build".to_string()),
            formats: default_formats(),
            html: HtmlConfig::default(),
            pdf: PdfConfig::default(),
            epub: EpubConfig::default(),
        }
    }
}

/// PDF spine configuration for merging multiple files into a single PDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfSpine {
    /// Title of the merged PDF document.
    /// Required when merge=true.
    pub title: Option<String>,

    /// Glob patterns for files to include in the combined document.
    /// Patterns are evaluated relative to content_dir (or project root if content_dir not set).
    /// Results are sorted lexicographically.
    /// Example: ["cover.typ", "chapters/**"]
    pub vertebrae: Vec<String>,

    /// Whether to merge vertebrae into a single PDF file.
    /// If false or not specified, compiles each file separately.
    #[serde(default)]
    pub merge: Option<bool>,
}

/// EPUB spine configuration for combining multiple files into a single EPUB.
/// EPUB always merges files - there is no merge option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpubSpine {
    /// Title of the EPUB document.
    /// Required for EPUB output.
    pub title: Option<String>,

    /// Glob patterns for files to include in the EPUB.
    /// Patterns are evaluated relative to content_dir (or project root if content_dir not set).
    /// Results are sorted lexicographically.
    /// Example: ["cover.typ", "chapters/**"]
    pub vertebrae: Vec<String>,
}

/// HTML spine configuration for organizing multiple HTML files.
/// HTML always produces per-file output - there is no merge option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtmlSpine {
    /// Title of the HTML site/collection.
    pub title: Option<String>,

    /// Glob patterns for files to include.
    /// Patterns are evaluated relative to content_dir (or project root if content_dir not set).
    /// Results are sorted lexicographically.
    /// Example: ["index.typ", "pages/**"]
    pub vertebrae: Vec<String>,
}

/// Common interface for spine configurations across output formats.
///
/// This trait provides uniform access to spine fields, allowing generic
/// code to work with any spine type (PDF, EPUB, HTML).
pub trait SpineConfig {
    /// Returns the spine title, if configured.
    fn title(&self) -> Option<&str>;

    /// Returns the vertebrae glob patterns.
    fn vertebrae(&self) -> &[String];

    /// Returns whether to merge outputs into a single file.
    /// Only meaningful for PDF; returns None for other formats.
    fn merge(&self) -> Option<bool> {
        None
    }
}

impl SpineConfig for PdfSpine {
    fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    fn vertebrae(&self) -> &[String] {
        &self.vertebrae
    }

    fn merge(&self) -> Option<bool> {
        self.merge
    }
}

impl SpineConfig for EpubSpine {
    fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    fn vertebrae(&self) -> &[String] {
        &self.vertebrae
    }
}

impl SpineConfig for HtmlSpine {
    fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    fn vertebrae(&self) -> &[String] {
        &self.vertebrae
    }
}

/// HTML output configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HtmlConfig {
    /// Stylesheet paths to inject (relative to build dir)
    #[serde(default = "default_stylesheets")]
    pub stylesheets: Vec<String>,

    /// Font URLs to inject
    #[serde(default = "default_fonts")]
    pub fonts: Vec<String>,

    /// Configuration for an HTML spine (sitemap/navbar).
    /// HTML never merges vertebrae.
    #[serde(default)]
    pub spine: Option<HtmlSpine>,
}

impl Default for HtmlConfig {
    fn default() -> Self {
        Self {
            stylesheets: default_stylesheets(),
            fonts: default_fonts(),
            spine: None,
        }
    }
}

/// PDF output configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PdfConfig {
    /// Configuration for a PDF spine with multiple chapters.
    pub spine: Option<PdfSpine>,
}

/// EPUB output configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EpubConfig {
    /// Unique global identifier for the EPUB document.
    ///
    /// See: EPUB 3.3, The `dc:identifier` element <https://www.w3.org/TR/epub-33/#sec-opf-dcidentifier>
    pub identifier: Option<String>,

    /// Publication date for the EPUB document.
    ///
    /// Note that this is separate from the timestamp indicating when a document was last modified,
    /// which is automatically generated by Rheo.
    ///
    /// See: EPUB 3.3, The `dc:date` element <https://www.w3.org/TR/epub-33/#sec-opf-dcdate>
    pub date: Option<DateTime<Utc>>,

    /// Configuration for an EPUB spine with multiple chapters.
    pub spine: Option<EpubSpine>,
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

        debug!(path = %config_path.display(), "loading configuration");
        let contents = std::fs::read_to_string(&config_path)
            .map_err(|e| crate::RheoError::io(e, format!("reading {}", config_path.display())))?;

        let config: RheoConfig = toml::from_str(&contents)
            .map_err(|e| crate::RheoError::project_config(format!("invalid rheo.toml: {}", e)))?;

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Load configuration from a specific path with validation
    ///
    /// # Errors
    /// Returns error if:
    /// - File does not exist
    /// - Path points to a directory, not a file
    /// - File is not valid TOML
    /// - TOML doesn't match RheoConfig schema
    pub fn load_from_path(config_path: &Path) -> Result<Self> {
        // Stage 1: File existence
        if !config_path.exists() {
            return Err(crate::RheoError::path(
                config_path,
                "config file does not exist",
            ));
        }
        if !config_path.is_file() {
            return Err(crate::RheoError::path(
                config_path,
                "config path must be a file, not a directory",
            ));
        }

        // Stage 2: Read file
        let contents = std::fs::read_to_string(config_path).map_err(|e| {
            crate::RheoError::io(e, format!("reading config file {}", config_path.display()))
        })?;

        // Stage 3: Parse TOML and validate schema
        let config: RheoConfig = toml::from_str(&contents).map_err(|e| {
            crate::RheoError::project_config(format!(
                "invalid config file {}: {}",
                config_path.display(),
                e
            ))
        })?;

        // Stage 4: Validate configuration
        config.validate()?;

        debug!(path = %config_path.display(), "loaded custom configuration");
        Ok(config)
    }

    /// Resolve content_dir to an absolute path if configured
    ///
    /// # Arguments
    /// * `base_dir` - Base directory to resolve content_dir against.
    ///   In directory mode: the project root directory
    ///   In single-file mode: the parent directory of the .typ file
    ///
    /// # Returns
    /// - Some(PathBuf) if content_dir is configured (absolute path)
    /// - None if content_dir is not set in config
    pub fn resolve_content_dir(&self, base_dir: &Path) -> Option<std::path::PathBuf> {
        self.content_dir.as_ref().map(|dir| {
            let path = base_dir.join(dir);
            debug!(content_dir = %path.display(), "resolved content directory");
            path
        })
    }

    pub fn has_pdf(&self) -> bool {
        self.formats.contains(&OutputFormat::Pdf)
    }

    pub fn has_html(&self) -> bool {
        self.formats.contains(&OutputFormat::Html)
    }

    pub fn has_epub(&self) -> bool {
        self.formats.contains(&OutputFormat::Epub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RheoConfig::default();
        assert_eq!(config.formats, OutputFormat::all_variants());
        assert_eq!(config.version, ManifestVersion::current());
    }

    #[test]
    fn test_config_missing_version_field() {
        let toml = r#"
        content_dir = "content"
        formats = ["pdf"]
        "#;

        let result = toml::from_str::<RheoConfig>(toml);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("missing field") || err_msg.contains("version"));
    }

    #[test]
    fn test_formats_from_config() {
        let toml = r#"
        version = "0.1.0"
        formats = ["pdf"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.formats, vec![OutputFormat::Pdf]);
    }

    #[test]
    fn test_formats_defaults_when_not_specified() {
        let toml = r#"
        version = "0.1.0"
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.formats, OutputFormat::all_variants());
    }

    #[test]
    fn test_formats_multiple_values() {
        let toml = r#"
        version = "0.1.0"
        formats = ["html", "epub"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.formats, vec![OutputFormat::Html, OutputFormat::Epub]);
    }

    #[test]
    fn test_formats_case_insensitive() {
        let toml = r#"
        version = "0.1.0"
        formats = ["PDF", "Html", "ePub"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        assert_eq!(
            config.formats,
            vec![OutputFormat::Pdf, OutputFormat::Html, OutputFormat::Epub]
        );
    }

    #[test]
    fn test_formats_invalid_format_name() {
        let toml = r#"
        version = "0.1.0"
        formats = ["invalid"]
        "#;

        let result = toml::from_str::<RheoConfig>(toml);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("unknown variant"));
    }

    #[test]
    fn test_load_from_path_not_found() {
        use std::path::PathBuf;

        let path = PathBuf::from("/tmp/nonexistent_config_12345_rheo_test.toml");
        let result = RheoConfig::load_from_path(&path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("config file does not exist"));
    }

    #[test]
    fn test_load_from_path_is_directory() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let result = RheoConfig::load_from_path(temp.path());
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("must be a file, not a directory"));
    }

    #[test]
    fn test_load_from_path_invalid_toml() {
        use std::fs;
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("invalid.toml");
        fs::write(&config_path, "[this is not valid toml").unwrap();

        let result = RheoConfig::load_from_path(&config_path);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("invalid config file"));
    }

    #[test]
    fn test_html_config_defaults() {
        let config = HtmlConfig::default();
        assert_eq!(config.stylesheets, vec!["style.css"]);
        assert_eq!(config.fonts.len(), 0);
    }

    #[test]
    fn test_html_config_custom_stylesheets() {
        let toml = r#"
        version = "0.1.0"
        [html]
        stylesheets = ["custom.css", "theme.css"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.html.stylesheets, vec!["custom.css", "theme.css"]);
        // Fonts should use default (empty)
        assert_eq!(config.html.fonts.len(), 0);
    }

    #[test]
    fn test_html_config_custom_fonts() {
        let toml = r#"
        version = "0.1.0"
        [html]
        fonts = ["https://example.com/font.css"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.html.fonts, vec!["https://example.com/font.css"]);
        // Stylesheets should use default
        assert_eq!(config.html.stylesheets, vec!["style.css"]);
    }

    #[test]
    fn test_html_config_both_custom() {
        let toml = r#"
        version = "0.1.0"
        [html]
        stylesheets = ["a.css", "b.css"]
        fonts = ["https://fonts.com/font1.css", "https://fonts.com/font2.css"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.html.stylesheets, vec!["a.css", "b.css"]);
        assert_eq!(
            config.html.fonts,
            vec!["https://fonts.com/font1.css", "https://fonts.com/font2.css"]
        );
    }

    #[test]
    fn test_pdf_spine_with_merge_true() {
        let toml = r#"
        version = "0.1.0"
        [pdf.spine]
        title = "My Book"
        vertebrae = ["cover.typ", "chapters/*.typ"]
        merge = true
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let spine = config.pdf.spine.as_ref().unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "My Book");
        assert_eq!(spine.vertebrae, vec!["cover.typ", "chapters/*.typ"]);
        assert_eq!(spine.merge, Some(true));
    }

    #[test]
    fn test_pdf_spine_with_merge_false() {
        let toml = r#"
        version = "0.1.0"
        [pdf.spine]
        title = "My Book"
        vertebrae = ["cover.typ", "chapters/*.typ"]
        merge = false
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let spine = config.pdf.spine.as_ref().unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "My Book");
        assert_eq!(spine.vertebrae, vec!["cover.typ", "chapters/*.typ"]);
        assert_eq!(spine.merge, Some(false));
    }

    #[test]
    fn test_pdf_spine_merge_omitted() {
        let toml = r#"
        version = "0.1.0"
        [pdf.spine]
        title = "My Book"
        vertebrae = ["cover.typ"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let spine = config.pdf.spine.as_ref().unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "My Book");
        assert_eq!(spine.vertebrae, vec!["cover.typ"]);
        assert_eq!(spine.merge, None);
    }

    #[test]
    fn test_epub_spine() {
        let toml = r#"
        version = "0.1.0"
        [epub.spine]
        title = "My EPUB"
        vertebrae = ["intro.typ", "chapter*.typ", "outro.typ"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let spine = config.epub.spine.as_ref().unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "My EPUB");
        assert_eq!(
            spine.vertebrae,
            vec!["intro.typ", "chapter*.typ", "outro.typ"]
        );
        // assert_eq!(spine.merge, None);
    }

    #[test]
    fn test_html_spine() {
        let toml = r#"
        version = "0.1.0"
        [html.spine]
        title = "My Website"
        vertebrae = ["index.typ", "about.typ"]
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let spine = config.html.spine.as_ref().unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "My Website");
        assert_eq!(spine.vertebrae, vec!["index.typ", "about.typ"]);
        // Note: HtmlSpine has no merge field - HTML always produces per-file output
    }

    #[test]
    fn test_spine_empty_vertebrae() {
        let toml = r#"
        version = "0.1.0"
        [epub.spine]
        title = "Single File Book"
        vertebrae = []
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let spine = config.epub.spine.as_ref().unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "Single File Book");
        assert!(spine.vertebrae.is_empty());
    }

    #[test]
    fn test_spine_complex_glob_patterns() {
        let toml = r#"
        version = "0.1.0"
        [pdf.spine]
        title = "Complex Book"
        vertebrae = ["frontmatter/**/*.typ", "chapters/**/ch*.typ", "appendix.typ"]
        merge = true
        "#;

        let config: RheoConfig = toml::from_str(toml).unwrap();
        let spine = config.pdf.spine.as_ref().unwrap();
        assert_eq!(spine.vertebrae.len(), 3);
        assert_eq!(spine.vertebrae[0], "frontmatter/**/*.typ");
        assert_eq!(spine.vertebrae[1], "chapters/**/ch*.typ");
        assert_eq!(spine.vertebrae[2], "appendix.typ");
    }
}

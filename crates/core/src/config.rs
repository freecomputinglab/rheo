use crate::Result;
use crate::manifest_version::ManifestVersion;
use crate::validation::ValidateConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::debug;

/// Spine configuration from `rheo.toml`: glob patterns, title, and optional merge flag.
///
/// All format plugins share this single config type. Each plugin interprets the
/// `merge` field according to its own defaults (e.g. EPUB defaults to merge=true).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Spine {
    /// Title for the merged output document (required when merge=true).
    pub title: Option<String>,

    /// Glob patterns for files to include, evaluated relative to content_dir.
    /// Results are sorted lexicographically within each pattern.
    /// Empty = auto-discover all .typ files.
    #[serde(default)]
    pub vertebrae: Vec<String>,

    /// Whether to merge vertebrae into a single output file.
    /// `None` means "use the plugin's default" (false for PDF/HTML, true for EPUB).
    pub merge: Option<bool>,
}

/// Plugin section for `[plugin_name]` in rheo.toml.
///
/// Contains the universal `spine` field plus format-specific extra fields in
/// `extra`. Each plugin reads only the keys it knows about from `extra`; unknown
/// keys are silently ignored. Adding a new plugin requires no changes to this
/// struct.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginSection {
    /// Spine configuration (shared by all plugins).
    pub spine: Option<Spine>,

    /// Per-plugin glob patterns for files to copy into this plugin's output directory.
    /// Paths are relative to the project root; directory structure is preserved.
    #[serde(default)]
    pub assets: Vec<String>,

    /// Plugin-specific extra fields from the TOML section (e.g. `stylesheets`,
    /// `fonts` for HTML; `identifier`, `date` for EPUB).
    #[serde(flatten, default)]
    pub extra: toml::Table,
}

/// Configuration for rheo compilation.
///
/// Loaded from `rheo.toml`. Unknown top-level table sections are parsed as
/// `PluginSection` entries (keyed by section name), so adding a new format
/// plugin requires no changes to this struct.
#[derive(Debug, Clone)]
pub struct RheoConfig {
    /// Manifest version for API compatibility (required).
    pub version: ManifestVersion,

    /// Directory containing .typ content files (relative to project root).
    pub content_dir: Option<String>,

    /// Build output directory (relative to project root unless absolute).
    pub build_dir: Option<String>,

    /// Default formats to compile when no CLI flags are specified.
    /// Empty = fall back to all registered plugins.
    pub formats: Vec<String>,

    /// Global glob patterns for files to copy into every plugin's output directory.
    /// Paths are relative to the project root; directory structure is preserved.
    pub assets: Vec<String>,

    /// Per-plugin configuration sections, keyed by plugin name.
    /// Built from `[html]`, `[pdf]`, `[epub]` (and any other) table sections.
    pub plugin_sections: HashMap<String, PluginSection>,
}

impl Default for RheoConfig {
    fn default() -> Self {
        Self {
            version: ManifestVersion::current(),
            content_dir: Some("./".to_string()),
            build_dir: Some("./build".to_string()),
            formats: vec![],
            assets: vec![],
            plugin_sections: HashMap::new(),
        }
    }
}

/// Raw intermediate for custom deserialization of `RheoConfig`.
#[derive(Debug, Deserialize)]
pub struct RheoConfigRaw {
    version: ManifestVersion,
    content_dir: Option<String>,
    build_dir: Option<String>,
    #[serde(default)]
    formats: Vec<String>,
    #[serde(default)]
    assets: Vec<String>,
    #[serde(flatten)]
    extra: HashMap<String, toml::Value>,
}

impl TryFrom<RheoConfigRaw> for RheoConfig {
    type Error = toml::de::Error;

    fn try_from(raw: RheoConfigRaw) -> std::result::Result<Self, Self::Error> {
        let mut plugin_sections = HashMap::new();
        for (key, value) in raw.extra {
            if let toml::Value::Table(_) = &value {
                let section: PluginSection = value.try_into()?;
                plugin_sections.insert(key, section);
            }
            // Non-table entries (unknown scalar fields) are silently ignored.
        }
        Ok(RheoConfig {
            version: raw.version,
            content_dir: raw.content_dir,
            build_dir: raw.build_dir,
            formats: raw.formats,
            assets: raw.assets,
            plugin_sections,
        })
    }
}

impl RheoConfig {
    /// Load configuration from rheo.toml in the given directory.
    /// If the file doesn't exist, returns default configuration.
    pub fn load(project_root: &Path) -> Result<Self> {
        let config_path = project_root.join("rheo.toml");

        if !config_path.exists() {
            debug!(path = %config_path.display(), "no rheo.toml found, using defaults");
            return Ok(Self::default());
        }

        debug!(path = %config_path.display(), "loading configuration");
        Self::parse_config(&config_path, "rheo.toml")
    }

    /// Load configuration from a specific path with validation.
    pub fn load_from_path(config_path: &Path) -> Result<Self> {
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

        let config = Self::parse_config(config_path, "config file")?;
        debug!(path = %config_path.display(), "loaded custom configuration");
        Ok(config)
    }

    /// Read, parse, convert, and validate a config file.
    fn parse_config(config_path: &Path, label: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(config_path)
            .map_err(|e| crate::RheoError::io(e, format!("reading {}", config_path.display())))?;

        let raw: RheoConfigRaw = toml::from_str(&contents)
            .map_err(|e| crate::RheoError::project_config(format!("invalid {}: {}", label, e)))?;
        let config = RheoConfig::try_from(raw)
            .map_err(|e| crate::RheoError::project_config(format!("invalid {}: {}", label, e)))?;

        config.validate()?;
        Ok(config)
    }

    /// Resolve content_dir to an absolute path if configured.
    pub fn resolve_content_dir(&self, base_dir: &Path) -> Option<std::path::PathBuf> {
        self.content_dir.as_ref().map(|dir| {
            let path = base_dir.join(dir);
            debug!(content_dir = %path.display(), "resolved content directory");
            path
        })
    }

    /// Returns true if `name` appears in the configured formats list.
    pub fn has_format(&self, name: &str) -> bool {
        self.formats.iter().any(|f| f == name)
    }

    /// Return the spine config for the named plugin, if any.
    pub fn spine_for_plugin(&self, name: &str) -> Option<&Spine> {
        self.plugin_sections
            .get(name)
            .and_then(|s| s.spine.as_ref())
    }

    /// Return the full plugin section for the named plugin.
    /// Returns `PluginSection::default()` if no section is configured.
    pub fn plugin_section(&self, name: &str) -> PluginSection {
        self.plugin_sections.get(name).cloned().unwrap_or_default()
    }
}

impl PluginSection {
    /// Get a string value from extra config, returning None if absent or not a string.
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.extra.get(key).and_then(|v| v.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a test TOML string with the current crate version prepended.
    fn versioned_toml(rest: &str) -> String {
        format!("version = \"{}\"\n{}", env!("CARGO_PKG_VERSION"), rest)
    }

    fn parse(toml: &str) -> RheoConfig {
        let raw: RheoConfigRaw = toml::from_str(toml).expect("parse failed");
        RheoConfig::try_from(raw).expect("convert failed")
    }

    #[test]
    fn test_default_config() {
        let config = RheoConfig::default();
        // formats is empty by default — CLI falls back to all_plugins()
        assert!(config.formats.is_empty());
        assert_eq!(config.version, ManifestVersion::current());
    }

    #[test]
    fn test_config_missing_version_field() {
        let toml = r#"
        content_dir = "content"
        formats = ["pdf"]
        "#;

        let result = toml::from_str::<RheoConfigRaw>(toml);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("missing field") || err_msg.contains("version"));
    }

    #[test]
    fn test_formats_from_config() {
        let toml = versioned_toml(r#"formats = ["pdf"]"#);
        let config = parse(&toml);
        assert_eq!(config.formats, vec!["pdf"]);
    }

    #[test]
    fn test_formats_defaults_when_not_specified() {
        let toml = versioned_toml("");
        let config = parse(&toml);
        // When not specified, formats is empty (CLI falls back to all_plugins())
        assert!(config.formats.is_empty());
    }

    #[test]
    fn test_formats_multiple_values() {
        let toml = versioned_toml(r#"formats = ["html", "epub"]"#);
        let config = parse(&toml);
        assert_eq!(config.formats, vec!["html", "epub"]);
    }

    #[test]
    fn test_formats_stored_as_given() {
        let toml = versioned_toml(r#"formats = ["pdf", "html", "epub"]"#);
        let config = parse(&toml);
        assert_eq!(config.formats, vec!["pdf", "html", "epub"]);
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
    fn test_html_section_defaults() {
        // When no [html] section, plugin_section("html") returns default (empty extra)
        let config = RheoConfig::default();
        let section = config.plugin_section("html");
        assert!(section.spine.is_none());
        assert!(section.extra.is_empty());
    }

    #[test]
    fn test_html_section_custom_stylesheets() {
        let toml = versioned_toml("[html]\nstylesheets = [\"custom.css\", \"theme.css\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        let sheets = section
            .extra
            .get("stylesheets")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(sheets.len(), 2);
        assert_eq!(sheets[0].as_str().unwrap(), "custom.css");
        assert_eq!(sheets[1].as_str().unwrap(), "theme.css");
    }

    #[test]
    fn test_html_section_custom_fonts() {
        let toml = versioned_toml("[html]\nfonts = [\"https://example.com/font.css\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        let fonts = section
            .extra
            .get("fonts")
            .and_then(|v| v.as_array())
            .unwrap();
        assert_eq!(fonts[0].as_str().unwrap(), "https://example.com/font.css");
    }

    #[test]
    fn test_pdf_spine_with_merge_true() {
        let toml = versioned_toml(
            "[pdf.spine]\ntitle = \"My Book\"\nvertebrae = [\"cover.typ\", \"chapters/*.typ\"]\nmerge = true",
        );
        let config = parse(&toml);
        let spine = config.spine_for_plugin("pdf").unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "My Book");
        assert_eq!(spine.vertebrae, vec!["cover.typ", "chapters/*.typ"]);
        assert_eq!(spine.merge, Some(true));
    }

    #[test]
    fn test_pdf_spine_with_merge_false() {
        let toml = versioned_toml(
            "[pdf.spine]\ntitle = \"My Book\"\nvertebrae = [\"cover.typ\", \"chapters/*.typ\"]\nmerge = false",
        );
        let config = parse(&toml);
        let spine = config.spine_for_plugin("pdf").unwrap();
        assert_eq!(spine.merge, Some(false));
    }

    #[test]
    fn test_pdf_spine_merge_omitted() {
        let toml = versioned_toml("[pdf.spine]\ntitle = \"My Book\"\nvertebrae = [\"cover.typ\"]");
        let config = parse(&toml);
        let spine = config.spine_for_plugin("pdf").unwrap();
        assert_eq!(spine.merge, None);
    }

    #[test]
    fn test_epub_spine() {
        let toml = versioned_toml(
            "[epub.spine]\ntitle = \"My EPUB\"\nvertebrae = [\"intro.typ\", \"chapter*.typ\", \"outro.typ\"]",
        );
        let config = parse(&toml);
        let spine = config.spine_for_plugin("epub").unwrap();
        assert_eq!(spine.title.as_deref().unwrap(), "My EPUB");
        assert_eq!(
            spine.vertebrae,
            vec!["intro.typ", "chapter*.typ", "outro.typ"]
        );
    }

    #[test]
    fn test_html_spine() {
        let toml = versioned_toml(
            "[html.spine]\ntitle = \"My Website\"\nvertebrae = [\"index.typ\", \"about.typ\"]",
        );
        let config = parse(&toml);
        let spine = config.spine_for_plugin("html").unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "My Website");
        assert_eq!(spine.vertebrae, vec!["index.typ", "about.typ"]);
    }

    #[test]
    fn test_spine_empty_vertebrae() {
        let toml = versioned_toml("[epub.spine]\ntitle = \"Single File Book\"\nvertebrae = []");
        let config = parse(&toml);
        let spine = config.spine_for_plugin("epub").unwrap();
        assert_eq!(spine.title.as_deref().unwrap(), "Single File Book");
        assert!(spine.vertebrae.is_empty());
    }

    #[test]
    fn test_spine_complex_glob_patterns() {
        let toml = versioned_toml(
            "[pdf.spine]\ntitle = \"Complex Book\"\nvertebrae = [\"frontmatter/**/*.typ\", \"chapters/**/ch*.typ\", \"appendix.typ\"]\nmerge = true",
        );
        let config = parse(&toml);
        let spine = config.spine_for_plugin("pdf").unwrap();
        assert_eq!(spine.vertebrae.len(), 3);
        assert_eq!(spine.vertebrae[0], "frontmatter/**/*.typ");
        assert_eq!(spine.vertebrae[1], "chapters/**/ch*.typ");
        assert_eq!(spine.vertebrae[2], "appendix.typ");
    }

    #[test]
    fn test_has_format() {
        let toml = versioned_toml(r#"formats = ["html", "pdf"]"#);
        let config = parse(&toml);
        assert!(config.has_format("html"));
        assert!(config.has_format("pdf"));
        assert!(!config.has_format("epub"));
    }

    #[test]
    fn test_epub_identifier_and_date() {
        let toml =
            versioned_toml("[epub]\nidentifier = \"urn:uuid:12345\"\ndate = 2025-01-15T00:00:00Z");
        let config = parse(&toml);
        let section = config.plugin_section("epub");
        assert_eq!(
            section.extra.get("identifier").and_then(|v| v.as_str()),
            Some("urn:uuid:12345")
        );
        assert!(section.extra.get("date").is_some());
    }

    #[test]
    fn test_global_assets_parses() {
        let toml = versioned_toml(r#"assets = ["*.txt", "assets/**/*.png"]"#);
        let config = parse(&toml);
        assert_eq!(config.assets, vec!["*.txt", "assets/**/*.png"]);
    }

    #[test]
    fn test_global_assets_defaults_empty() {
        let toml = versioned_toml("");
        let config = parse(&toml);
        assert!(config.assets.is_empty());
    }

    #[test]
    fn test_plugin_copy_parses() {
        let toml = versioned_toml("[html]\nassets = [\"assets/logo.png\", \"fonts/**\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert_eq!(section.assets, vec!["assets/logo.png", "fonts/**"]);
    }

    #[test]
    fn test_plugin_copy_not_in_extra() {
        let toml = versioned_toml("[html]\nassets = [\"assets/logo.png\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        // `assets` must be in the dedicated field, not leaked into `extra`
        assert!(section.extra.get("assets").is_none());
    }

    #[test]
    fn test_plugin_copy_defaults_empty() {
        let toml = versioned_toml("[html]\nstylesheets = [\"style.css\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert!(section.assets.is_empty());
    }
}

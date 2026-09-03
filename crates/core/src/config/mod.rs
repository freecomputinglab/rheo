use crate::Result;
use serde::{Deserialize, Deserializer};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

pub mod manifest_version;
pub mod packages;
pub mod retired;

pub use manifest_version::ManifestVersion;
pub use packages::{GitRef, NamespaceSource, PathSource, ReleasesSource, RepoSource};
use retired::warn_on_retired_keys;
pub use retired::{RETIRED_KEYS, RetiredKey};

/// The one `sys.inputs` key rheo owns. A project may not set it from
/// `--input`/`[inputs]`: a forged spine or target would be indistinguishable
/// from the real one to every package reading it.
pub const RESERVED_INPUT_KEY: &str = "rheo-context";

/// A `bool` config key whose absence means `DEFAULT` — one type for every
/// "unset means this" toggle, in place of an `Option<bool>` field paired with
/// its own `unwrap_or` accessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flag<const DEFAULT: bool>(Option<bool>);

impl<const DEFAULT: bool> Flag<DEFAULT> {
    /// The configured value, or `DEFAULT` when the key is unset.
    pub fn get(&self) -> bool {
        self.0.unwrap_or(DEFAULT)
    }
}

impl<const DEFAULT: bool> From<bool> for Flag<DEFAULT> {
    fn from(value: bool) -> Self {
        Self(Some(value))
    }
}

impl<'de, const DEFAULT: bool> Deserialize<'de> for Flag<DEFAULT> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Option::<bool>::deserialize(deserializer).map(Flag)
    }
}

/// One format's resolved spine knobs: every field already merged over the
/// global `[spine]` table. See [`Spine::merged_over`].
pub struct MergedSpine {
    pub exclude: Vec<String>,
    pub layering: Option<SpineLayering>,
    pub title: Option<String>,
}

/// How a spine lays out its leaves beyond the plain directory scan.
///
/// `include` is a flat reorder and `section` nests into virtual directories, so
/// a table sets one or the other — an enum rather than two `Option` fields
/// because "both at once" names nothing.
#[derive(Debug, Clone)]
pub enum SpineLayering {
    /// Virtual directories: each section groups matched files under a named
    /// node without moving them on disk.
    Sections(Vec<SpineSection>),
    /// Ordered glob list replacing this spine's scan order, dropping any leaf
    /// it does not match, without [`SpineLayering::Sections`]' group nesting.
    Include(Vec<String>),
}

/// Spine configuration from `rheo.toml`: directory-scan knobs and title.
///
/// All format plugins share this single config type.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(try_from = "SpineRaw")]
pub struct Spine {
    /// Title for the combined output document, when applicable.
    pub title: Option<String>,

    /// Glob patterns (relative to content_dir) for files/folders to omit from
    /// the directory-scanned spine.
    ///
    /// `None` when unset in this table (as opposed to `Some(vec![])`, an
    /// explicit empty list) so a per-format `[plugin.spine]` that doesn't set
    /// `exclude` falls back to the global `[spine] exclude` field-by-field,
    /// rather than a per-format table's mere presence blanking every global
    /// spine key at once (rheo-9vl.2).
    pub exclude: Option<Vec<String>>,

    /// `section` or `include`, whichever this table set; `None` when neither,
    /// for the same per-field fallback reason as `exclude`.
    pub layering: Option<SpineLayering>,

    /// Unrecognized keys, captured so [`warn_on_retired_keys`] can warn when a
    /// field retired from `Spine` in a past version (e.g. the removed
    /// `vertebrae` glob list) is still set in an older `rheo.toml`, rather than
    /// silently dropping it.
    pub extra: toml::Table,
}

/// The `[spine]` keys as written, before the `section`/`include` one-of rule.
#[derive(Debug, Deserialize)]
pub struct SpineRaw {
    title: Option<String>,
    #[serde(default)]
    exclude: Option<Vec<String>>,
    #[serde(default)]
    section: Option<Vec<SpineSection>>,
    #[serde(default)]
    include: Option<Vec<String>>,
    #[serde(flatten, default)]
    extra: toml::Table,
}

impl TryFrom<SpineRaw> for Spine {
    type Error = toml::de::Error;

    fn try_from(raw: SpineRaw) -> std::result::Result<Self, Self::Error> {
        let layering = match (raw.section, raw.include) {
            (Some(_), Some(_)) => {
                return Err(serde::de::Error::custom(
                    "include is a flat reorder, section nests into virtual \
                     directories: set one, not both",
                ));
            }
            (Some(section), None) => Some(SpineLayering::Sections(section)),
            (None, Some(include)) => Some(SpineLayering::Include(include)),
            (None, None) => None,
        };
        Ok(Spine {
            title: raw.title,
            exclude: raw.exclude,
            layering,
            extra: raw.extra,
        })
    }
}

/// A virtual directory in the spine: groups flat files under a named node,
/// behaving like an on-disk subdirectory. Nests to arbitrary depth via `section`.
#[derive(Debug, Clone, Deserialize)]
pub struct SpineSection {
    /// Slug — the node's handle segment and sibling sort key (like a dir name).
    pub name: String,
    /// Display title; when absent, derived from `name`.
    pub title: Option<String>,
    /// Glob patterns (relative to content_dir) for files pulled under this section.
    #[serde(default)]
    pub include: Vec<String>,
    /// Nested virtual directories.
    #[serde(default)]
    pub section: Vec<SpineSection>,
}

/// Asset configuration for `[plugin_name.assets]` in rheo.toml.
///
/// Holds glob copy patterns (`copy`), an optional destination subdirectory
/// (`dest`), and AssetConfig path overrides (any other key). Separating these
/// into their own subtable ensures AssetConfig names cannot clash with other
/// `[plugin_name]` fields like `spine`.
#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
pub struct PluginAssets {
    /// Glob patterns for files to copy into this plugin's output directory.
    /// Paths are relative to the project root; directory structure is preserved.
    #[serde(default)]
    pub copy: Vec<String>,

    /// Optional destination subdirectory (relative to plugin output dir)
    /// for every file produced by this block. When set:
    /// - named asset overrides (e.g. `js_scripts`) are placed at
    ///   `<plugin_output_dir>/<dest>/<basename>` (source directory stripped);
    /// - `copy` glob matches are placed at
    ///   `<plugin_output_dir>/<dest>/<rel>` where `<rel>` is the source's
    ///   project-root-relative path (structure preserved).
    #[serde(default)]
    pub dest: Option<String>,

    /// AssetConfig path overrides, keyed by the AssetConfig name
    /// (e.g. `css_stylesheet = "custom.css"`).
    #[serde(flatten, default)]
    pub extra: toml::Table,
}

/// Accepts either `[plugin.assets]` (single table) or `[[plugin.assets]]` (array-of-tables).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AssetsField {
    Single(PluginAssets),
    Multiple(Vec<PluginAssets>),
}

impl AssetsField {
    /// Normalised view: a slice of asset blocks regardless of source syntax.
    pub fn blocks(&self) -> &[PluginAssets] {
        match self {
            AssetsField::Single(a) => std::slice::from_ref(a),
            AssetsField::Multiple(v) => v.as_slice(),
        }
    }
}

/// Plugin section for `[plugin_name]` in rheo.toml.
///
/// Contains the universal `spine` field plus format-specific extra fields in
/// `extra`. Each plugin reads only the keys it knows about from `extra`; unknown
/// keys are silently ignored. Adding a new plugin requires no changes to this
/// struct.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PluginSection {
    /// Spine configuration (shared by all plugins).
    pub spine: Option<Spine>,

    /// Asset configuration subtable (`[plugin_name.assets]`).
    /// Holds glob copy patterns and AssetConfig path overrides.
    pub assets: Option<AssetsField>,

    /// When true (default), auto-detect package assets from `#import "@ns/name:ver"`
    /// statements in .typ files by reading each package's `typst.toml` `[tool.rheo.*]`.
    /// Set to false to disable import-driven asset injection for this format.
    #[serde(default)]
    pub auto_detect_packages: Flag<true>,

    /// When true (default), per-page output for this format resets the footnote
    /// counter to 1 on every page; false lets footnotes accumulate across the
    /// bundle. Only meaningful for the per-page formats (HTML/EPUB); PDF combines
    /// into a single document with no `ext`, so the reset gate in `typ/rheo.typ`
    /// never fires there regardless.
    #[serde(default)]
    pub reset_footnotes: Flag<true>,

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
    pub copy: Vec<String>,

    /// Directories to search for fonts (relative to project root unless absolute).
    pub font_dirs: Vec<String>,

    /// Per-plugin configuration sections, keyed by plugin name.
    /// Built from `[html]`, `[pdf]`, `[epub]` (and any other) table sections.
    pub plugin_sections: HashMap<String, PluginSection>,

    /// Global spine configuration (applies when no per-plugin spine is set).
    pub spine: Option<Spine>,

    /// Filename, relative to `content_dir`, whose Typst is inlined at the
    /// bundle root instead of being compiled as a vertebra. Defaults to
    /// [`crate::MARROW_FILE`] when unset.
    ///
    /// This names the *project's* marrow only. An imported package always
    /// contributes its own `.marrow.typ`, so renaming this cannot suppress a
    /// package's contribution or vice versa — both are inlined.
    pub marrow: Option<String>,

    /// When true, the project's own marrow is spliced BEFORE every document
    /// instead of after, so a `#show`/`#set` rule in it reaches pre-existing
    /// vertebrae. Defaults to `false` (today's behaviour) — prologue is
    /// global-by-default and powerful, so it is opt-in only. A package
    /// declares its own marrow's position by filename instead (`.marrow.typ`
    /// vs `.marrow-prologue.typ`); this key affects only the project's marrow.
    pub marrow_prologue: Flag<false>,

    /// `[inputs]` — project-declared `sys.inputs` keys for the Typst compile.
    ///
    /// Typst has no environment access, so `sys.inputs` is the only channel by
    /// which a project can parameterise a compile. Declaring them here is the
    /// baseline; `--input KEY=VALUE` overrides an individual key at the command
    /// line. Combined with `--config rheo.public.toml` this gives per-variant
    /// input sets with no flags at all. `@rheo/rookery`'s `exclude-tags` is the
    /// first consumer.
    ///
    /// VALUES ARE STRINGS, always, and a non-string is a config ERROR rather than
    /// a coercion: `typst compile --input` hands Typst a string, so a package
    /// written against one spelling must not behave differently under the other.
    /// TOML makes quoting cheap, so `foo = "3"` is no hardship.
    ///
    /// `rheo-context` is REJECTED here, as it is on the CLI — rheo owns that key,
    /// and a project able to overwrite it could hand every package a forged spine.
    pub inputs: HashMap<String, String>,

    /// `[packages.<namespace>]` — where each declared package namespace resolves
    /// from, keyed by namespace.
    ///
    /// Empty is the ordinary case and means today's behaviour exactly: `@rheo`
    /// from its built-in releases host, everything else from Typst universe. An
    /// entry for `rheo` overrides the built-in, which is how a project tests a
    /// branch of rheo-packages.
    pub packages: HashMap<String, NamespaceSource>,
}

impl Spine {
    /// Merge a per-format spine table over the global one, each field falling
    /// back INDEPENDENTLY when unset — so `[pdf.spine] title` alone still
    /// inherits the global `exclude`, rather than the per-format table's mere
    /// presence blanking every global spine key at once.
    pub fn merged_over(this: Option<&Spine>, global: Option<&Spine>) -> MergedSpine {
        fn pick<T: Clone>(
            this: Option<&Spine>,
            global: Option<&Spine>,
            field: impl Fn(&Spine) -> Option<&T>,
        ) -> Option<T> {
            this.and_then(&field)
                .or_else(|| global.and_then(&field))
                .cloned()
        }

        MergedSpine {
            exclude: pick(this, global, |s| s.exclude.as_ref()).unwrap_or_default(),
            layering: pick(this, global, |s| s.layering.as_ref()),
            title: pick(this, global, |s| s.title.as_ref()),
        }
    }
}

impl Default for RheoConfig {
    fn default() -> Self {
        Self {
            version: ManifestVersion::current(),
            content_dir: Some("./".to_string()),
            build_dir: Some("./build".to_string()),
            formats: vec![],
            copy: vec![],
            font_dirs: vec![],
            plugin_sections: HashMap::new(),
            spine: None,
            marrow: None,
            marrow_prologue: Flag::default(),
            inputs: HashMap::new(),
            packages: HashMap::new(),
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
    copy: Vec<String>,
    #[serde(default)]
    font_dirs: Vec<String>,
    marrow: Option<String>,
    #[serde(default)]
    marrow_prologue: Flag<false>,
    #[serde(flatten)]
    extra: HashMap<String, toml::Value>,
}

impl TryFrom<RheoConfigRaw> for RheoConfig {
    type Error = toml::de::Error;

    fn try_from(mut raw: RheoConfigRaw) -> std::result::Result<Self, Self::Error> {
        let spine: Option<Spine> = match raw.extra.remove("spine") {
            Some(value) => value.try_into()?,
            None => None,
        };
        // PULLED OUT OF `extra` BEFORE the plugin-section loop below, exactly as
        // `spine` is, and this is load-bearing rather than stylistic. That loop
        // turns EVERY unknown table into a `PluginSection` keyed by its name — so
        // without this line an `[inputs]` table does not error, does not warn, and
        // becomes a phantom plugin section named `inputs` that nothing ever reads.
        // Silently doing nothing is the worst available outcome, which is why the
        // test below asserts `plugin_sections` does not contain the key.
        //
        // `try_into()` to `HashMap<String, String>` gives the string-only policy
        // for free, with toml's own error naming the key and the expected type
        // ("invalid type: integer, expected a string"), so there is no hand-rolled
        // type check to keep in step with serde's.
        let inputs: HashMap<String, String> = match raw.extra.remove("inputs") {
            Some(value) => value.try_into()?,
            None => HashMap::new(),
        };
        if inputs.contains_key(RESERVED_INPUT_KEY) {
            return Err(serde::de::Error::custom(format!(
                "[inputs] may not set `{RESERVED_INPUT_KEY}`: rheo owns that key, which \
                 carries the spine and the output format that every package reads. Choose \
                 another name, e.g. a package-prefixed one like `rookery-exclude`.",
            )));
        }
        // Pulled out before the plugin-section loop for the same reason `inputs`
        // is: left in `extra` it becomes a plugin section named `packages` that
        // nothing reads, and no namespace ever resolves.
        let packages = match raw.extra.remove("packages") {
            Some(value) => NamespaceSource::parse_table(value)?,
            None => HashMap::new(),
        };
        let mut plugin_sections = HashMap::new();
        for (key, value) in raw.extra {
            if let toml::Value::Table(_) = &value {
                let section: PluginSection = value.try_into()?;
                plugin_sections.insert(key, section);
            }
            // Non-table entries (unknown scalar fields) are silently ignored.
        }

        let current = ManifestVersion::current();
        if raw.version != current {
            warn!(
                "rheo.toml version {} does not match rheo version {}. Run `rheo migrate` to \
                 update it (add --apply to write the changes).",
                raw.version, current
            );
        }
        // Naming the table each key was authored in, so a per-format table's
        // retired key does not send the reader to the global one.
        if let Some(spine) = &spine {
            warn_on_retired_keys("[spine]", &spine.extra);
        }
        for (name, section) in &plugin_sections {
            warn_on_retired_keys(&format!("[{name}]"), &section.extra);
            if let Some(spine) = &section.spine {
                warn_on_retired_keys(&format!("[{name}.spine]"), &spine.extra);
            }
        }

        Ok(RheoConfig {
            version: raw.version,
            content_dir: raw.content_dir,
            build_dir: raw.build_dir,
            formats: raw.formats,
            copy: raw.copy,
            font_dirs: raw.font_dirs,
            plugin_sections,
            spine,
            marrow: raw.marrow,
            marrow_prologue: raw.marrow_prologue,
            inputs,
            packages,
        })
    }
}

/// Anchor a config-declared directory against the project root. An absolute
/// `dir` wins, as `Path::join` defines.
fn resolve_dir(base_dir: &Path, dir: &str, kind: &str) -> PathBuf {
    let path = base_dir.join(dir);
    debug!(kind, dir = %path.display(), "resolved directory");
    path
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
        Self::parse_config(&config_path, "rheo.toml", project_root)
    }

    /// Load configuration from a specific path.
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

        let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        let config = Self::parse_config(config_path, "config file", base_dir)?;
        debug!(path = %config_path.display(), "loaded custom configuration");
        Ok(config)
    }

    /// Read and parse a config file. Invalid state is rejected by
    /// [`RheoConfig::try_from`], so a parsed config needs no further pass.
    ///
    /// `base_dir` is the directory a relative `[packages.<ns>] path` resolves
    /// against — the config file's own directory, not the process's cwd, so
    /// `path = "../rookery"` means the same thing however rheo was invoked.
    fn parse_config(config_path: &Path, label: &str, base_dir: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(config_path)
            .map_err(|e| crate::RheoError::io(e, format!("reading {}", config_path.display())))?;

        let raw: RheoConfigRaw = toml::from_str(&contents)
            .map_err(|e| crate::RheoError::project_config(format!("invalid {}: {}", label, e)))?;
        let mut config = RheoConfig::try_from(raw)
            .map_err(|e| crate::RheoError::project_config(format!("invalid {}: {}", label, e)))?;

        for source in config.packages.values_mut() {
            source.anchor_to(base_dir);
        }

        Ok(config)
    }

    /// The project's marrow filename, relative to `content_dir`.
    pub fn marrow_file(&self) -> &str {
        self.marrow.as_deref().unwrap_or(crate::MARROW_FILE)
    }

    /// Resolve content_dir against the project root, if configured.
    pub fn resolve_content_dir(&self, base_dir: &Path) -> Option<PathBuf> {
        self.content_dir
            .as_ref()
            .map(|dir| resolve_dir(base_dir, dir, "content"))
    }

    /// Resolve all font_dirs against the project root.
    pub fn resolve_font_dirs(&self, base_dir: &Path) -> Vec<PathBuf> {
        self.font_dirs
            .iter()
            .map(|dir| resolve_dir(base_dir, dir, "font"))
            .collect()
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
    /// Returns the asset blocks, normalised to a slice regardless of source syntax.
    pub fn asset_blocks(&self) -> &[PluginAssets] {
        self.assets.as_ref().map(|a| a.blocks()).unwrap_or(&[])
    }

    /// Deserialize the format-specific `extra` fields into a typed config struct.
    ///
    /// Plugins define a `#[derive(Deserialize, Default)]` struct for their own
    /// keys and call `ctx.config.parse_extra::<MyConfig>()?` instead of hand-rolling
    /// `extra.get("k").and_then(|v| v.as_str())` lookups. Unknown keys are ignored,
    /// so each plugin only declares the fields it reads.
    pub fn parse_extra<T: serde::de::DeserializeOwned>(&self) -> Result<T> {
        toml::Value::Table(self.extra.clone())
            .try_into()
            .map_err(|e| crate::RheoError::project_config(format!("invalid plugin config: {e}")))
    }

    /// Get a string value from the `[plugin.assets]` overrides, returning None if absent or not a string.
    /// Returns the first override found across all asset blocks.
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.asset_blocks()
            .iter()
            .find_map(|b| b.extra.get(key).and_then(|v| v.as_str()))
    }

    /// Collect every override for `key` across all asset blocks, in declared order.
    pub fn get_strings(&self, key: &str) -> Vec<&str> {
        self.asset_blocks()
            .iter()
            .filter_map(|b| b.extra.get(key).and_then(|v| v.as_str()))
            .collect()
    }

    /// Collect every (block, override-value) pair for `key` across all
    /// asset blocks, in declared order. Used by callers that need each
    /// block's `dest` to compute output paths.
    pub fn get_strings_with_block(&self, key: &str) -> Vec<(&PluginAssets, &str)> {
        self.asset_blocks()
            .iter()
            .filter_map(|b| b.extra.get(key).and_then(|v| v.as_str()).map(|s| (b, s)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a test TOML string with the current crate version prepended.
    fn versioned_toml(rest: &str) -> String {
        format!("version = \"{}\"\n{}", env!("CARGO_PKG_VERSION"), rest)
    }

    /// The sections of a spine's layering; every caller here set `section`.
    fn sections(spine: &Spine) -> &[SpineSection] {
        match spine.layering.as_ref() {
            Some(SpineLayering::Sections(sections)) => sections,
            other => panic!("expected sections, got {other:?}"),
        }
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
    fn test_reset_footnotes_defaults_true_and_honors_false_per_format() {
        // No [html] section at all -> default section, defaults to true.
        let config = parse(&versioned_toml(r#"formats = ["html"]"#));
        assert!(config.plugin_section("html").reset_footnotes.get());

        // Explicit false on [html] is honored, and is per-format: [epub] still
        // defaults to true.
        let config = parse(&versioned_toml("[html]\nreset_footnotes = false\n[epub]\n"));
        assert!(!config.plugin_section("html").reset_footnotes.get());
        assert!(config.plugin_section("epub").reset_footnotes.get());

        // Explicit true.
        let config = parse(&versioned_toml("[html]\nreset_footnotes = true\n"));
        assert!(config.plugin_section("html").reset_footnotes.get());
    }

    #[test]
    fn test_merged_spine_falls_back_field_by_field() {
        let global = Spine {
            exclude: Some(vec!["drafts/**".to_string()]),
            title: Some("Global".to_string()),
            ..Default::default()
        };
        // A per-format table that sets only `title` must still inherit the
        // global `exclude` — its mere presence must not blank it.
        let per_format = Spine {
            title: Some("Book".to_string()),
            ..Default::default()
        };

        let merged = Spine::merged_over(Some(&per_format), Some(&global));
        assert_eq!(merged.title.as_deref(), Some("Book"));
        assert_eq!(merged.exclude, vec!["drafts/**".to_string()]);

        // A field the per-format table DOES set wins outright.
        let overriding = Spine {
            exclude: Some(vec!["other/**".to_string()]),
            ..Default::default()
        };
        let merged = Spine::merged_over(Some(&overriding), Some(&global));
        assert_eq!(merged.exclude, vec!["other/**".to_string()]);
        assert_eq!(merged.title.as_deref(), Some("Global"));

        // Neither table present: empty lists and no title.
        let merged = Spine::merged_over(None, None);
        assert!(merged.exclude.is_empty());
        assert!(merged.layering.is_none());
        assert_eq!(merged.title, None);
    }

    #[test]
    fn test_marrow_prologue_defaults_false_and_honors_true() {
        // No key at all -> defaults to epilogue (today's behaviour).
        let config = parse(&versioned_toml(""));
        assert!(!config.marrow_prologue.get());

        let config = parse(&versioned_toml("marrow_prologue = true"));
        assert!(config.marrow_prologue.get());

        let config = parse(&versioned_toml("marrow_prologue = false"));
        assert!(!config.marrow_prologue.get());
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
        // When no [html] section, plugin_section("html") returns default (no assets)
        let config = RheoConfig::default();
        let section = config.plugin_section("html");
        assert!(section.spine.is_none());
        assert!(section.assets.is_none());
        assert!(section.extra.is_empty());
    }

    #[test]
    fn test_html_section_custom_stylesheets() {
        let toml = versioned_toml("[html]\nstylesheets = [\"custom.css\", \"theme.css\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        // stylesheets is a non-asset extra field, still in PluginSection.extra
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
    fn test_pdf_spine_parses_title() {
        // A legacy `merge` key (removed) is silently ignored, so old configs
        // still parse; title is read as usual.
        let toml = versioned_toml("[pdf.spine]\ntitle = \"My Book\"\nmerge = true");
        let config = parse(&toml);
        let spine = config.spine_for_plugin("pdf").unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "My Book");
    }

    #[test]
    fn test_epub_spine() {
        let toml = versioned_toml("[epub.spine]\ntitle = \"My EPUB\"");
        let config = parse(&toml);
        let spine = config.spine_for_plugin("epub").unwrap();
        assert_eq!(spine.title.as_deref().unwrap(), "My EPUB");
    }

    #[test]
    fn test_html_spine() {
        let toml = versioned_toml("[html.spine]\ntitle = \"My Website\"");
        let config = parse(&toml);
        let spine = config.spine_for_plugin("html").unwrap();
        assert_eq!(spine.title.as_ref().unwrap(), "My Website");
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
    fn test_global_copy_parses() {
        let toml = versioned_toml(r#"copy = ["*.txt", "assets/**/*.png"]"#);
        let config = parse(&toml);
        assert_eq!(config.copy, vec!["*.txt", "assets/**/*.png"]);
    }

    #[test]
    fn test_global_copy_defaults_empty() {
        let toml = versioned_toml("");
        let config = parse(&toml);
        assert!(config.copy.is_empty());
    }

    #[test]
    fn test_plugin_copy_parses() {
        let toml = versioned_toml("[html.assets]\ncopy = [\"assets/logo.png\", \"fonts/**\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert_eq!(
            section.asset_blocks()[0].copy,
            vec!["assets/logo.png", "fonts/**"]
        );
    }

    #[test]
    fn test_plugin_copy_not_in_extra() {
        let toml = versioned_toml("[html.assets]\ncopy = [\"assets/logo.png\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        // `copy` must be in PluginAssets.copy, not leaked into PluginAssets.extra
        assert!(section.asset_blocks()[0].extra.get("copy").is_none());
    }

    #[test]
    fn test_plugin_copy_defaults_empty() {
        let toml = versioned_toml("[html]\nstylesheets = [\"style.css\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert!(section.assets.is_none());
    }

    #[test]
    fn test_get_string_returns_string_value() {
        let toml = versioned_toml("[html.assets]\ncss_stylesheet = \"custom.css\"");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert_eq!(section.get_string("css_stylesheet"), Some("custom.css"));
    }

    #[test]
    fn test_get_string_returns_none_for_missing_key() {
        let toml = versioned_toml("[html]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert_eq!(section.get_string("nonexistent"), None);
    }

    #[test]
    fn test_get_string_returns_none_for_non_string_value() {
        let toml = versioned_toml("[html.assets]\ncss_stylesheet = [\"a\", \"b\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert_eq!(section.get_string("css_stylesheet"), None);
    }

    #[test]
    fn test_font_dirs_parses() {
        let toml = versioned_toml(r#"font_dirs = ["fonts", "custom/typefaces"]"#);
        let config = parse(&toml);
        assert_eq!(config.font_dirs, vec!["fonts", "custom/typefaces"]);
    }

    #[test]
    fn test_font_dirs_defaults_empty() {
        let toml = versioned_toml("");
        let config = parse(&toml);
        assert!(config.font_dirs.is_empty());
    }

    #[test]
    fn test_resolve_font_dirs_resolves_relative() {
        use std::path::PathBuf;

        let toml = versioned_toml(r#"font_dirs = ["fonts"]"#);
        let config = parse(&toml);
        let base_dir = PathBuf::from("/project");
        let resolved = config.resolve_font_dirs(&base_dir);
        assert_eq!(resolved, vec![PathBuf::from("/project/fonts")]);
    }

    #[test]
    fn test_assets_array_of_tables_parses() {
        let toml = versioned_toml(
            "[[html.assets]]\njs_scripts = \"one.js\"\n[[html.assets]]\njs_scripts = \"two.js\"",
        );
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert_eq!(section.asset_blocks().len(), 2);
        assert_eq!(section.get_strings("js_scripts"), vec!["one.js", "two.js"]);
    }

    #[test]
    fn test_assets_single_table_still_parses() {
        let toml = versioned_toml("[html.assets]\njs_scripts = \"one.js\"");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert_eq!(section.asset_blocks().len(), 1);
        assert_eq!(section.get_string("js_scripts"), Some("one.js"));
    }

    #[test]
    fn test_get_strings_collects_across_blocks() {
        let toml = versioned_toml(
            "[[html.assets]]\ncss_stylesheet = \"a.css\"\n[[html.assets]]\ncss_stylesheet = \"b.css\"",
        );
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert_eq!(
            section.get_strings("css_stylesheet"),
            vec!["a.css", "b.css"]
        );
    }

    #[test]
    fn test_get_string_returns_first_match_across_blocks() {
        let toml = versioned_toml(
            "[[html.assets]]\ncss_stylesheet = \"first.css\"\n[[html.assets]]\ncss_stylesheet = \"second.css\"",
        );
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert_eq!(section.get_string("css_stylesheet"), Some("first.css"));
    }

    #[test]
    fn test_asset_blocks_empty_when_no_assets_field() {
        let toml = versioned_toml("[html]\nstylesheets = [\"style.css\"]");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert!(section.asset_blocks().is_empty());
    }

    #[test]
    fn test_dest_field_parses() {
        let toml =
            versioned_toml("[html.assets]\ndest = \"allassets\"\ncss_stylesheet = \"custom.css\"");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        let blocks = section.asset_blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].dest.as_deref(), Some("allassets"));
    }

    #[test]
    fn test_dest_defaults_to_none() {
        let toml = versioned_toml("[html.assets]\ncss_stylesheet = \"custom.css\"");
        let config = parse(&toml);
        let section = config.plugin_section("html");
        assert!(section.asset_blocks()[0].dest.is_none());
    }

    #[test]
    fn test_get_strings_with_block_returns_pairs() {
        let toml = versioned_toml(
            "[[html.assets]]\njs_scripts = \"one.js\"\ndest = \"subdir\"\n[[html.assets]]\njs_scripts = \"two.js\"",
        );
        let config = parse(&toml);
        let section = config.plugin_section("html");
        let pairs = section.get_strings_with_block("js_scripts");
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].1, "one.js");
        assert_eq!(pairs[0].0.dest.as_deref(), Some("subdir"));
        assert_eq!(pairs[1].1, "two.js");
        assert!(pairs[1].0.dest.is_none());
    }

    #[test]
    fn test_global_spine_exclude_and_nested_sections() {
        let toml = versioned_toml(
            r#"
        [spine]
        exclude = ["drafts/**", "*.tmp.typ"]

        [[spine.section]]
        name = "guides"

        [[spine.section.section]]
        name = "advanced"
        "#,
        );
        let config = parse(&toml);
        let spine = config.spine.as_ref().unwrap();
        assert_eq!(
            spine.exclude.clone().unwrap(),
            vec!["drafts/**", "*.tmp.typ"]
        );
        let sections = sections(spine);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "guides");
        assert_eq!(sections[0].section.len(), 1);
        assert_eq!(sections[0].section[0].name, "advanced");
        assert!(!config.plugin_sections.contains_key("spine"));
    }

    #[test]
    fn test_pdf_spine_exclude_and_section() {
        let toml = versioned_toml(
            r#"
        [pdf.spine]
        exclude = ["scratch/**"]

        [[pdf.spine.section]]
        name = "appendix"
        include = ["appendix/*.typ"]
        "#,
        );
        let config = parse(&toml);
        let spine = config.spine_for_plugin("pdf").unwrap();
        assert_eq!(spine.exclude.clone().unwrap(), vec!["scratch/**"]);
        let sections = sections(spine);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].name, "appendix");
        assert_eq!(sections[0].include, vec!["appendix/*.typ"]);
    }

    #[test]
    fn test_legacy_vertebrae_only_config_still_parses() {
        // A legacy config setting only the retired `vertebrae` key still
        // parses (captured in `extra`, not a typed field) rather than erroring.
        let toml = versioned_toml("[pdf.spine]\nvertebrae = [\"cover.typ\", \"chapters/*.typ\"]");
        let config = parse(&toml);
        let spine = config.spine_for_plugin("pdf").unwrap();
        assert!(spine.extra.contains_key("vertebrae"));
        assert!(spine.exclude.is_none());
        assert!(spine.layering.is_none());
    }

    /// `include` and `section` in one table is rejected where the table is
    /// parsed, so no later pass can be skipped and no `Spine` value can carry
    /// both at once.
    #[test]
    fn test_spine_rejects_include_with_section() {
        let toml =
            versioned_toml("[spine]\ninclude = [\"a.typ\"]\n\n[[spine.section]]\nname = \"g\"");
        let raw: RheoConfigRaw = toml::from_str(&toml).expect("raw parse should succeed");
        let err = RheoConfig::try_from(raw).expect_err("both knobs at once must fail");
        assert!(
            err.to_string().contains("flat reorder"),
            "error should explain the two knobs, got: {err}",
        );
    }

    #[test]
    fn test_spine_include_alone_is_a_flat_reorder() {
        let config = parse(&versioned_toml("[spine]\ninclude = [\"b.typ\", \"a.typ\"]"));
        let spine = config.spine.as_ref().unwrap();
        let Some(SpineLayering::Include(include)) = &spine.layering else {
            panic!("expected a flat include, got {:?}", spine.layering);
        };
        assert_eq!(include, &vec!["b.typ".to_string(), "a.typ".to_string()]);
    }

    /// A retired key (and an unrecognized one) is a warning at most: an old
    /// `rheo.toml` still parses, in the global table and a per-format one alike.
    #[test]
    fn test_retired_and_unknown_keys_still_parse() {
        let config = parse(&versioned_toml(
            "[spine]\nvertebrae = [\"[invalid\"]\nmerge = true\nsome_future_key = \"x\"\n\
             [pdf.spine]\nmerge = true\n[html]\nfeed_title = \"gone\"\n",
        ));
        assert!(config.spine.as_ref().unwrap().extra.contains_key("merge"));
        assert!(
            config
                .spine_for_plugin("pdf")
                .unwrap()
                .extra
                .contains_key("merge")
        );
    }

    /// A version that does not match the running rheo warns; it never fails the
    /// parse, or the project could not be read by the tool that migrates it.
    #[test]
    fn test_mismatched_manifest_version_still_parses() {
        let raw: RheoConfigRaw = toml::from_str("version = \"0.0.1\"").unwrap();
        assert!(RheoConfig::try_from(raw).is_ok());
        let raw: RheoConfigRaw = toml::from_str("version = \"99.0.0\"").unwrap();
        assert!(RheoConfig::try_from(raw).is_ok());
    }

    /// `[inputs]` reaches `RheoConfig::inputs` and — the part this test exists
    /// for — does NOT become a phantom plugin section.
    ///
    /// THE REGRESSION GUARD. `RheoConfigRaw` carries `#[serde(flatten)] extra`,
    /// and the `TryFrom` loop turns every unknown TABLE into a `PluginSection`
    /// keyed by its name. Before `extra.remove("inputs")`, an `[inputs]` table
    /// therefore parsed cleanly, warned about nothing, and was read by nobody. A
    /// feature that silently does nothing is exactly what a test has to pin.
    #[test]
    fn test_inputs_table_is_read_and_not_a_plugin_section() {
        let config = parse(&versioned_toml(
            r#"
        [inputs]
        rookery-exclude = "private,protected"
        empty-is-fine = ""
    "#,
        ));
        assert_eq!(
            config.inputs.get("rookery-exclude").map(String::as_str),
            Some("private,protected"),
        );
        assert_eq!(
            config.inputs.get("empty-is-fine").map(String::as_str),
            Some("")
        );
        assert!(
            !config.plugin_sections.contains_key("inputs"),
            "[inputs] was swallowed as a phantom plugin section",
        );
    }

    /// No `[inputs]` table at all still loads, with an empty map — the common case.
    #[test]
    fn test_inputs_table_absent_is_empty() {
        let config = parse(&versioned_toml("content_dir = \"content\"\n"));
        assert!(config.inputs.is_empty());
    }

    /// A non-string value is a config ERROR, not a coercion. `typst compile
    /// --input` hands Typst a string, so rheo must too, or a package written
    /// against one spelling misbehaves under the other.
    #[test]
    fn test_inputs_rejects_non_string_value() {
        let raw: RheoConfigRaw = toml::from_str(&versioned_toml(
            r#"
        [inputs]
        foo = 3
    "#,
        ))
        .expect("raw parse should succeed — the value is only checked on convert");
        let err = RheoConfig::try_from(raw).expect_err("a non-string input must fail");
        let msg = err.to_string();
        assert!(
            msg.contains("string"),
            "error should name the expected type, got: {msg}",
        );
    }

    /// `rheo-context` is rheo's own key. A project able to set it could hand every
    /// package a forged spine, so both entry points reject it.
    #[test]
    fn test_inputs_rejects_reserved_key() {
        let raw: RheoConfigRaw = toml::from_str(&versioned_toml(
            r#"
        [inputs]
        rheo-context = "forged"
    "#,
        ))
        .expect("raw parse should succeed");
        let err = RheoConfig::try_from(raw).expect_err("the reserved key must fail");
        assert!(
            err.to_string().contains("rheo-context"),
            "error should name the reserved key, got: {err}",
        );
    }

    #[test]
    fn test_spine_section_title_defaults_to_none() {
        let toml = r#"
        name = "guides"
        include = ["guides/*.typ"]
    "#;
        let section: SpineSection = toml::from_str(toml).expect("parse failed");
        assert_eq!(section.name, "guides");
        assert!(section.title.is_none());
        assert!(section.include.len() == 1);
        assert!(section.section.is_empty());
    }

    /// A relative `[packages.<ns>] path` resolves against the CONFIG FILE'S OWN
    /// directory, not the process's cwd — so it means the same thing however
    /// rheo was invoked.
    #[test]
    fn test_relative_package_path_resolves_against_config_dir() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("rheo.toml");
        std::fs::write(
            &config_path,
            versioned_toml("[packages.ns]\npath = \"../pkgs\"\n"),
        )
        .unwrap();

        let config = RheoConfig::load_from_path(&config_path).expect("load failed");
        let NamespaceSource::Path(source) = config.packages.get("ns").expect("namespace absent")
        else {
            panic!("expected a path source");
        };
        assert_eq!(source.root, temp.path().join("../pkgs"));
    }

    /// An absolute `path` passes through unchanged.
    #[test]
    fn test_absolute_package_path_is_left_unchanged() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("rheo.toml");
        std::fs::write(
            &config_path,
            versioned_toml("[packages.ns]\npath = \"/abs/pkgs\"\n"),
        )
        .unwrap();

        let config = RheoConfig::load_from_path(&config_path).expect("load failed");
        let NamespaceSource::Path(source) = config.packages.get("ns").expect("namespace absent")
        else {
            panic!("expected a path source");
        };
        assert_eq!(source.root, std::path::PathBuf::from("/abs/pkgs"));
    }
}

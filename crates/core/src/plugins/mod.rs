use crate::config::PluginSection;
use crate::project::ProjectConfig;
use crate::reticulate::spine::SpineLayout;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;
use typst::foundations::Bytes;

pub mod typst_manifest;
pub use typst_manifest::{
    detect_manifest_package_assets, detect_manifest_package_assets_in_dirs, find_package_in_dirs,
    manifest_package_assets, prewarm_packages, scan_project_package_imports,
    typst_package_search_dirs,
};

/// Trait for managing a running preview server.
pub trait ServerHandle: Send + Sync {
    fn url(&self) -> &str;
    fn reload(&self);
    /// Push a new in-memory file system to the server (no-op by default).
    fn update_virtual_fs(&self, _vfs: typst_bundle::VirtualFs) {}
}

/// Handle returned by FormatPlugin::open() for managing the opened resource
pub enum OpenHandle {
    /// Server-based preview — usable via ServerHandle trait methods.
    Server(Box<dyn ServerHandle>),
    /// Direct file open (PDF/EPUB) - fire-and-forget, no reload needed
    Direct,
    /// No preview capability
    None,
}

use crate::config::PluginAssets;

/// Standardized spine options resolved by rheo core before calling compile().
#[derive(Debug, Clone)]
pub struct SpineOptions {
    pub title: Option<String>,
    pub vertebrae: Vec<String>,
}

/// Declares an additional non-Typst input file needed from the project directory.
#[derive(Debug, Clone)]
pub struct AssetConfig {
    /// Key used to retrieve this input from PluginContext::inputs
    pub name: &'static str,
    /// Default path relative to the project root (not the content directory) where the file is
    /// expected.
    pub default_path: &'static str,
    /// If true, a missing file is a compile error; if false, it is absent from ctx.inputs
    pub required: bool,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub config: AssetConfig,
    pub resolved_path: PathBuf,
    pub built_relative_path: String,
}

/// Template data contributed by a plugin for `rheo init`.
///
/// Returned by [`FormatPlugin::format_init_template`].
#[derive(Debug, Clone, Default)]
pub struct FormatInitTemplate {
    /// `(relative_path, content)` pairs written verbatim by `rheo init`.
    pub files: Vec<(&'static str, &'static str)>,
    /// TOML snippet embedded under `[<plugin>.*]` in the generated `rheo.toml`.
    pub options_toml: Option<&'static str>,
}

/// A single output produced by one Typst spine compilation.
///
/// Each item maps an output filename (relative to the plugin output dir) to the
/// raw bytes emitted by the compiler (e.g. HTML string bytes, PDF bytes).
pub struct CastVertebra {
    /// Output filename relative to the plugin output dir (e.g., `"chapter1.html"`).
    pub output_path: String,
    /// Raw bytes from the Typst compiler.
    pub bytes: Bytes,
    /// Typst compile target this output was produced with.
    pub format: TypstFormat,
    /// Harvested `rheo-*` variables from the vertebra's source file.
    pub vars: std::collections::HashMap<String, crate::reticulate::types::RheoValue>,
}

impl CastVertebra {
    /// Parse this output as an HTML DOM.
    ///
    /// Returns an error if `format` is not `TypstFormat::Html`.
    pub fn html(&self) -> crate::Result<crate::html_utils::HtmlDom> {
        if self.format != TypstFormat::Html {
            return Err(crate::RheoError::invalid_data("output is not HTML-shaped"));
        }
        crate::html_utils::HtmlDom::parse(&String::from_utf8_lossy(&self.bytes))
    }
}

/// Typst export target — the format argument passed to `#document(…, format: "…")`.
///
/// Distinct from `FormatPlugin::extension()`, which controls output filenames and
/// `@ref` anchors. EPUB uses `.xhtml` as its extension but compiles via the `html`
/// Typst target; PDF uses the `pdf` target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypstFormat {
    Pdf,
    Html,
    Png,
    Svg,
}

impl TypstFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Html => "html",
            Self::Png => "png",
            Self::Svg => "svg",
        }
    }
}

/// How the plugin wants the spine laid out for compilation.
///
/// Core uses this to synthesize the virtual main Typst source, which drives one
/// bundle compile that produces all outputs at once. The choice here replaces
/// the old `default_merge` / `spine.merge` flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpineLayoutKind {
    /// One output file per vertebra — each gets its own `#document(…)` block.
    /// Output filename is `<stem>.<plugin_extension>`.
    OnePerVertebra,
    /// All vertebrae in a single output — one `#document(…)` wrapping all includes.
    /// Output filename is `<project_name>.<plugin_extension>`.
    SingleCombined,
}

/// Context passed to plugin.compile() for each compilation unit.
pub struct PluginContext<'a> {
    pub project: &'a ProjectConfig,
    /// Plugin output directory (e.g., `build/html/`). Write outputs here.
    pub output_dir: &'a PathBuf,
    /// Resolved spine options (title, vertebrae patterns).
    pub spine: &'a SpineOptions,
    /// Full parsed plugin section from rheo.toml (or default if not configured).
    ///
    /// # Reading format-specific configuration
    ///
    /// Plugins define a typed config struct for their own keys and deserialize
    /// the whole section in one call with [`PluginSection::parse_extra`]:
    ///
    /// ```ignore
    /// #[derive(serde::Deserialize, Default)]
    /// struct EpubConfig {
    ///     identifier: Option<String>,
    ///     stylesheets: Vec<String>,
    /// }
    ///
    /// let cfg = ctx.config.parse_extra::<EpubConfig>()?;
    /// ```
    ///
    /// Unknown keys are ignored, so each plugin only declares the fields it reads.
    pub config: &'a PluginSection,
    /// Resolved additional input files declared by the plugin.
    ///
    /// Paths are relative to the plugin's output directory (e.g., `build/html/`).
    /// The CLI copies each declared input from the project root to the output directory
    /// before calling `compile()`.
    pub assets: &'a HashMap<&'static str, Vec<Asset>>,
    /// Additional font directories to search, resolved by the build (autoscan +
    /// config + `--font-dir`). Threaded into worlds the plugin creates if needed.
    pub font_dirs: &'a [PathBuf],
}

/// Parse `@namespace/name:version` into its components. Returns None on malformed input.
pub fn parse_package_spec(spec: &str) -> Option<(&str, &str, &str)> {
    let without_at = spec.strip_prefix('@')?;
    let slash = without_at.find('/')?;
    let namespace = &without_at[..slash];
    let rest = &without_at[slash + 1..];
    let colon = rest.rfind(':')?;
    let name = &rest[..colon];
    let version = &rest[colon + 1..];
    if namespace.is_empty() || name.is_empty() || version.is_empty() {
        return None;
    }
    Some((namespace, name, version))
}

/// A package expanded into synthetic asset blocks, carrying its resolved source root.
#[derive(Debug, PartialEq)]
pub struct PackageAssets {
    pub assets: PluginAssets,
    pub source_root: PathBuf,
}

/// A resolved package specifier ready for asset block synthesis.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPackage {
    pub name: String,
    pub source_root: PathBuf,
    pub namespace: Option<String>,
    pub version: Option<String>,
}

/// Plugin trait for implementing new output formats in rheo.
///
/// # Implementing a new plugin
///
/// To add a new output format to rheo:
///
/// 1. Create a new crate in `crates/` (e.g., `rheo-markdown`)
/// 2. Implement the `FormatPlugin` trait on a zero-sized type:
///    ```ignore
///    pub struct MarkdownPlugin;
///    impl FormatPlugin for MarkdownPlugin { ... }
///    ```
/// 3. Add the plugin to `all_plugins()` in `crates/cli/src/lib.rs`
/// 4. Add a `[markdown]` section to the rheo.toml configuration reference in CLAUDE.md
/// 5. Document format-specific configuration options in the CLAUDE.md config reference
///
/// # Plugin lifecycle
///
/// For each project compilation:
///
/// 1. **Load config**: rheo.toml is parsed (or defaults are used)
/// 2. **Apply defaults**: `apply_defaults()` is called if no config section exists
/// 3. **Resolve inputs**: Files declared by `inputs()` are copied to output directory
/// 4. **Compile**: rheo core runs a single Typst bundle compile and passes the outputs to `compile()`
/// 5. **Open**: `open()` is called if `--open` flag was passed
pub trait FormatPlugin: Send + Sync {
    /// Plugin identifier, CLI flag, and output subdirectory name.
    ///
    /// The return value serves triple duty:
    ///
    /// 1. **CLI flag**: `--<name>` enables this format (e.g., `--html`)
    /// 2. **Output subdirectory**: files are written to `build/<name>/` (e.g., `build/html/`)
    /// 3. **Format name**: passed to sys.inputs for `rheo-target()` polyfill injection
    ///
    /// **Requirements:**
    /// - Must be stable (do not change between versions)
    /// - Must be lowercase
    /// - Must be alphanumeric (underscores allowed)
    /// - Should be short and descriptive
    fn name(&self) -> &'static str;

    /// The extension used for output files; defaults to `name()`.
    fn extension(&self) -> &'static str {
        self.name()
    }

    /// How the plugin wants the spine laid out for compilation.
    ///
    /// Core converts this to a `SpineLayout`, synthesizes the virtual main, and
    /// runs one bundle compile. Plugins that produce one file per vertebra
    /// (HTML) use `OnePerVertebra`; plugins that merge everything into one file
    /// (PDF) use `SingleCombined`.
    fn spine_layout_kind(&self) -> SpineLayoutKind {
        SpineLayoutKind::OnePerVertebra
    }

    /// Typst export target emitted as the `format:` argument of `#document(…)`.
    ///
    /// Distinct from `extension()` (output filename / `@ref` anchor). Override when
    /// the output extension differs from the Typst compile target (e.g. EPUB uses
    /// `.xhtml` filenames but compiles via the `html` target).
    fn typst_format(&self) -> TypstFormat {
        TypstFormat::Html
    }

    /// The value injected as `sys.inputs.rheo-target`, surfaced to documents via
    /// the `target()` polyfill.
    ///
    /// This keeps formats that share a Typst export target distinguishable: EPUB
    /// and HTML both compile via the `html` target, but report `"epub"` and
    /// `"html"` respectively, so packages can branch on the rheo output format.
    ///
    /// Returning `None` injects nothing, leaving `target()` to fall back to
    /// Typst's `std.target()` — used by PDF, which wants the native `"paged"`.
    /// Defaults to `Some(name())`.
    fn rheo_target(&self) -> Option<&'static str> {
        Some(self.name())
    }

    /// Set plugin-specific smart defaults when no rheo.toml section exists.
    fn apply_defaults(&self, _section: &mut PluginSection, _project_name: &str) {}

    /// Open the output for this format in the appropriate viewer.
    fn open(&self, output_dir: &Path, _format_name: &str) -> crate::Result<OpenHandle> {
        open_all_files_in_folder(output_dir.to_path_buf(), self.name())?;
        Ok(OpenHandle::Direct)
    }

    /// Declare additional non-Typst input files this plugin needs.
    fn assets(&self) -> Vec<AssetConfig> {
        vec![]
    }

    /// Combined init template: files and TOML config section for `rheo init`.
    fn format_init_template(&self) -> FormatInitTemplate {
        FormatInitTemplate::default()
    }

    /// Provide Typst library code to inject into all compiled files.
    fn typst_library(&self) -> Option<&'static str> {
        None
    }

    /// Compile the spine to output files.
    ///
    /// `outputs` contains one entry per compiled document, in spine order.
    /// Each entry carries the output filename and raw bytes (HTML, PDF, etc.)
    /// as produced by the Typst compiler. The plugin reads, post-processes, and
    /// writes each to `ctx.output_dir`.
    ///
    /// # Error handling
    ///
    /// Return errors as `Err(...)` — the build records failures and continues with
    /// other plugins. Do not swallow errors silently.
    fn compile(&self, ctx: PluginContext<'_>, outputs: &[CastVertebra]) -> crate::Result<()>;
}

/// Open all files with a given extension in a folder using the OS default application.
pub(crate) fn open_all_files_in_folder(folder: PathBuf, ext: &str) -> crate::Result<()> {
    use walkdir::WalkDir;

    for entry in WalkDir::new(&folder)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some(ext))
    {
        let path = entry.path();
        info!("Opening: {}", path.display());

        if let Err(e) = opener::open(path) {
            tracing::warn!("Failed to open {}: {}", path.display(), e);
        }
    }

    Ok(())
}

/// Build a [`SpineLayout`] from a [`SpineLayoutKind`] and project context.
pub(crate) fn spine_layout_for(
    kind: SpineLayoutKind,
    plugin: &dyn FormatPlugin,
    project_name: &str,
) -> SpineLayout {
    let format = plugin.typst_format().as_str().to_string();
    match kind {
        SpineLayoutKind::OnePerVertebra => SpineLayout::OnePerVertebra {
            ext: plugin.extension().to_string(),
            format,
        },
        SpineLayoutKind::SingleCombined => SpineLayout::SingleCombined {
            output_name: format!("{}.{}", project_name, plugin.extension()),
            format,
        },
    }
}

use crate::config::PluginSection;
use crate::project::ProjectConfig;
use crate::reticulate::spine::{SpineLayout, VirtualSpine};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;
use typst::foundations::Bytes;

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

/// An embedded fallback for an asset: content the plugin ships built in, written
/// into the output directory (under `name`) when no on-disk source resolves, then
/// linked like any copied asset. Lets a plugin ship a default (e.g. the HTML
/// default stylesheet) as a real linked file rather than inlining it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedDefault {
    /// Output filename (relative to the plugin output dir) to write the content as.
    pub name: &'static str,
    /// The verbatim bytes to write.
    pub content: &'static str,
}

/// Declares an additional non-Typst input file needed from the project directory.
#[derive(Debug, Clone)]
pub struct AssetConfig {
    /// Key this asset is retrieved by from [`PageAssets::assets`]
    pub name: &'static str,
    /// Default path relative to the project root (not the content directory) where the file is
    /// expected.
    pub default_path: &'static str,
    /// If true, a missing file is a compile error; if false, it is absent from ctx.inputs
    pub required: bool,
    /// Embedded fallback written to the output dir and linked when no on-disk
    /// source (default path or override) resolves. `None` = no fallback.
    pub default_content: Option<EmbeddedDefault>,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub config: AssetConfig,
    /// Whether this script must be loaded as an ES module. Only ever true for a
    /// package's source-mode block, whose unbundled files use `import`.
    pub module: bool,
    /// Absolute path of the source file this asset was copied from.
    ///
    /// For user-declared assets this is under the project root; for
    /// package-declared assets it is under the package's `source_root`.
    pub source_path: PathBuf,
    /// Absolute path of the copied file in the plugin output directory.
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
    /// Resolved document title — Typst's own `DocumentInfo::title` for this
    /// output, read off the compiled bundle (see
    /// [`Build::compile_spine`](crate::build::Build)), falling back to the
    /// matching spine [`Vertebra`](crate::reticulate::spine::Vertebra)'s
    /// path-derived title when no `DocumentMeta` exists for this output.
    ///
    /// Empty when the output has no matching per-vertebra source at all (e.g.
    /// a combined output).
    pub title: String,
    /// Resolved `#set document(date:)` timestamp, if present.
    pub date: Option<chrono::DateTime<chrono::Utc>>,
    /// Resolved document description (`#set document(description: ..)`), if present.
    pub description: Option<String>,
    /// Resolved document keywords (`#set document(keywords: ..)`); empty when none were set.
    pub keywords: Vec<String>,
    /// Resolved document author(s) (`#set document(author: ..)`); empty when none were set.
    pub author: Vec<String>,
    /// True when this output has no matching spine [`Vertebra`](crate::reticulate::spine::Vertebra) —
    /// a page minted at the bundle root by a `.marrow.typ` contribution, or (for
    /// `SingleCombined` layouts) the merged multi-vertebra output. A plugin
    /// building a reading-order index over its outputs should exclude these;
    /// the output itself still belongs in the format's container.
    pub contributed: bool,
}

impl CastVertebra {
    /// Decode this output's bytes as a UTF-8 string.
    pub fn html_string(&self) -> crate::Result<String> {
        String::from_utf8(self.bytes.to_vec())
            .map_err(|e| crate::RheoError::invalid_data(format!("output is not valid UTF-8: {e}")))
    }

    /// Parse this output as an HTML DOM.
    ///
    /// Returns an error if `format` is not `TypstFormat::Html`.
    pub fn html(&self) -> crate::Result<crate::html_dom::HtmlDom> {
        if self.format != TypstFormat::Html {
            return Err(crate::RheoError::invalid_data("output is not HTML-shaped"));
        }
        crate::html_dom::HtmlDom::parse(&String::from_utf8_lossy(&self.bytes))
    }
}

/// Typst export target — the format argument passed to `#document(…, format: "…")`.
///
/// Distinct from `FormatPlugin::extension()`, which controls output filenames and
/// `@ref` anchors — a plugin's output extension need not match the Typst target
/// it actually compiles through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypstFormat {
    Pdf,
    Html,
}

impl TypstFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Html => "html",
        }
    }
}

/// How the plugin wants the spine laid out for compilation.
///
/// Core uses this to synthesize the virtual main Typst source, which drives one
/// bundle compile that produces all outputs at once. Each plugin declares
/// whether it emits one file per vertebra or a single combined file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpineLayoutKind {
    /// One output file per vertebra — each gets its own `#document(…)` block.
    /// Output filename is `<stem>.<plugin_extension>`.
    OnePerVertebra,
    /// All vertebrae in a single output — one `#document(…)` wrapping all includes.
    /// Output filename is `<project_name>.<plugin_extension>`.
    SingleCombined,
}

/// What a format needs to finish one of its pages: the assets core resolved for
/// this build, and the bundle's site-wide `<head>` contribution. A format that
/// writes plain files (PDF) reads neither.
#[derive(Clone, Copy)]
pub struct PageAssets<'a> {
    /// Resolved assets declared by [`FormatPlugin::assets`], keyed by
    /// [`AssetConfig::name`]. Paths are relative to the plugin's output
    /// directory; core has already copied each one there.
    pub assets: &'a HashMap<&'static str, Vec<Asset>>,
    /// The `.rheo/head.html` control asset's fragment, when the bundle minted
    /// one — content for *every* page's `<head>`, as opposed to a single page's
    /// own `<rheo-head>` wrapper.
    pub head_fragment: Option<&'a str>,
}

impl<'a> PageAssets<'a> {
    /// One compiled page of this build, ready for [`LiveReload::rewrite_page`].
    pub fn page(&self, path: &'a str, text: &'a str) -> ServedPage<'a> {
        ServedPage {
            path,
            text,
            assets: self.assets,
            head_fragment: self.head_fragment,
        }
    }
}

/// What a format needs to assemble the whole bundle into something other than
/// loose files — an EPUB's package document, its reading order, its embedded
/// assets. A format that writes each page as it comes (HTML, PDF) reads none of
/// it.
pub struct BundleInputs<'a> {
    pub project: &'a ProjectConfig,
    /// The resolved spine — the same tree and flat vertebra list as the
    /// Typst-side `rheo-context` (`spine`/`spine-flat`), plus the resolved
    /// combined-document title (`spine.title`, distinct from any individual
    /// vertebra's own title). [`FormatPlugin::compile`]'s
    /// `outputs: &[CastVertebra]` is a separate, already-cast view of each
    /// output's own title/date: use `spine` for structure and cross-vertebra
    /// queries, `outputs` for per-output compiled bytes.
    pub spine: &'a VirtualSpine,
    /// Bundle-emitted `asset()` bytes with no matching spine vertebra (e.g. a
    /// marrow contribution), keyed by their path relative to the plugin output
    /// directory. Core writes these as loose files unless
    /// [`FormatPlugin::embeds_bundle_assets`] returns `true`, in which case the
    /// plugin places them itself (see that method for why).
    pub assets: &'a [(String, Bytes)],
}

/// What core hands [`FormatPlugin::compile`]: where to write, this format's own
/// configuration, and one bundle per capability — page finishing and
/// whole-bundle assembly — so a signature stops implying every format reads
/// everything.
pub struct PluginContext<'a> {
    /// Plugin output directory (e.g. `build/html/`). Write outputs here.
    pub output_dir: &'a PathBuf,
    /// This format's parsed `rheo.toml` section (or the default when the
    /// project configures none).
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
    /// Unknown keys are ignored, so each plugin only declares what it reads.
    pub config: &'a PluginSection,
    pub page: PageAssets<'a>,
    pub bundle: BundleInputs<'a>,
}

/// One freshly compiled page, with everything a format needs to finish it
/// before it is served or written.
pub struct ServedPage<'a> {
    /// Output path relative to the plugin's output directory
    /// (e.g. `chapters/intro.html`), which fixes the page's link depth.
    pub path: &'a str,
    /// The compiled page, decoded.
    pub text: &'a str,
    /// Assets core resolved for this build, keyed by [`AssetConfig::name`].
    pub assets: &'a HashMap<&'static str, Vec<Asset>>,
    /// Site-wide `<head>` contribution from a `.rheo/head.html` control asset.
    pub head_fragment: Option<&'a str>,
}

/// Serving a format's pages from memory, for `rheo watch`'s dev server.
///
/// A format opting in ([`FormatPlugin::live_reload`]) gets each page of every
/// watch rebuild handed back for the same finishing it would do when writing to
/// disk, so the served bytes match the compiled ones. Core owns the compile,
/// the transclusion and the control assets; only this last per-format step is
/// dispatched here.
pub trait LiveReload: Send + Sync {
    /// Finish `page`, or return `None` to serve it exactly as compiled.
    fn rewrite_page(&self, page: &ServedPage<'_>) -> crate::Result<Option<String>>;
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
    /// Whether this block's scripts are ES modules (`js_module = true`).
    pub js_module: bool,
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
///
/// Under `rheo watch`, a format that returns [`Self::live_reload`] also has each
/// rebuilt page passed through [`LiveReload::rewrite_page`] before the dev
/// server serves it from memory. That is the whole of the dev-server contract:
/// core picks the serving format by this capability, never by plugin name.
pub trait FormatPlugin: Send + Sync {
    /// Plugin identifier, CLI flag, and output subdirectory name.
    ///
    /// The return value serves triple duty:
    ///
    /// 1. **CLI flag**: `--<name>` enables this format (e.g., `--html`)
    /// 2. **Output subdirectory**: files are written to `build/<name>/` (e.g., `build/html/`)
    /// 3. **Format name**: threaded into `rheo-context.target` for the `target()` polyfill
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
    /// runs one bundle compile. Use `OnePerVertebra` for a plugin that emits one
    /// output per vertebra; use `SingleCombined` for one that merges every
    /// vertebra into a single output.
    fn spine_layout_kind(&self) -> SpineLayoutKind {
        SpineLayoutKind::OnePerVertebra
    }

    /// Typst export target emitted as the `format:` argument of `#document(…)`.
    ///
    /// Distinct from `extension()` (output filename / `@ref` anchor). Override when
    /// a plugin's output extension differs from the Typst compile target it
    /// actually compiles through.
    fn typst_format(&self) -> TypstFormat {
        TypstFormat::Html
    }

    /// Whether this plugin takes over placing bundle-emitted `asset()` bytes
    /// ([`BundleInputs::assets`]) into its own output, instead of core
    /// writing them as loose files in the output directory.
    ///
    /// Default `false` — most plugins produce a directory of files where a
    /// loose asset sits usefully alongside the pages that reference it. A
    /// plugin whose output is a single container file overrides this to embed
    /// the bytes instead, since a loose file next to the container is
    /// unreachable from inside it.
    fn embeds_bundle_assets(&self) -> bool {
        false
    }

    /// The output-format name written into `sys.inputs.rheo-context.target`,
    /// surfaced to documents via the `target()` polyfill.
    ///
    /// This keeps formats that share a Typst export target distinguishable: two
    /// plugins compiling through the same target can still report different
    /// names here, so packages can branch on the rheo output format.
    ///
    /// Returning `None` injects nothing, leaving `target()` to fall back to
    /// Typst's native `std.target()` — for a plugin whose compile target
    /// already identifies it uniquely. Defaults to `Some(name())`.
    fn rheo_target(&self) -> Option<&'static str> {
        Some(self.name())
    }

    /// Set plugin-specific smart defaults when no rheo.toml section exists.
    fn apply_defaults(&self, _section: &mut PluginSection, _project_name: &str) {}

    /// This format's live-reload capability, when it has one.
    ///
    /// `Some` means `rheo watch` compiles this format into memory on every
    /// rebuild and serves it through the handle [`Self::open`] returns; the
    /// default `None` means the format is only ever written to disk.
    fn live_reload(&self) -> Option<&dyn LiveReload> {
        None
    }

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

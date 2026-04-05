use crate::compile::{compile_document_to_string, document_to_pdf_bytes};
use crate::config::PluginSection;
use crate::output::OutputConfig;
use crate::project::ProjectConfig;
use crate::world::RheoWorld;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use tracing::{debug, info};
use typst_html::HtmlDocument;

/// Trait for managing a running preview server.
pub trait ServerHandle: Send + Sync {
    fn url(&self) -> &str;
    fn reload(&self);
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

use crate::compile::RheoCompileOptions;
use crate::{BuiltSpine, Result, RheoError};

/// Standardized spine options resolved by rheo core before calling compile().
#[derive(Debug, Clone)]
pub struct SpineOptions {
    pub title: Option<String>,
    pub vertebrae: Vec<String>,
    /// true = merged output, false = per-file output
    pub merge: bool,
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

impl AssetConfig {
    /// Names that are reserved by `PluginSection` serde deserialization.
    /// An asset with one of these names would silently collide with
    /// `PluginSection::spine` or `PluginSection::assets`.
    pub const RESERVED_NAMES: &[&str] = &["spine", "assets"];

    /// Validate that this asset's name does not collide with reserved keys.
    pub fn validate(&self, plugin_name: &str) -> crate::Result<()> {
        if Self::RESERVED_NAMES.contains(&self.name) {
            return Err(crate::RheoError::misconfigured_plugin(format!(
                "plugin '{}' declares an asset named '{}' which conflicts \
                 with the reserved rheo.toml field; asset names must not be \
                 'spine' or 'assets'",
                plugin_name, self.name
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub config: AssetConfig,
    pub resolved_path: PathBuf,
    pub built_relative_path: String,
}

/// Context passed to plugin.compile() for each compilation unit.
pub struct PluginContext<'a> {
    pub project: &'a ProjectConfig,
    pub output_config: &'a OutputConfig,
    pub options: RheoCompileOptions<'a>,
    /// Resolved spine options (title, vertebrae, merge flag).
    pub spine: &'a SpineOptions,
    /// Full parsed plugin section from rheo.toml (or default if not configured).
    ///
    /// # Reading format-specific configuration
    ///
    /// Plugins read format-specific fields from `config.extra` using serde JSON patterns:
    ///
    /// ```ignore
    /// // Read a string value
    /// let identifier = section.extra.get("identifier")
    ///     .and_then(|v| v.as_str())
    ///     .map(String::from);
    ///
    /// // Read an array of strings
    /// let stylesheets: Vec<String> = section.extra.get("stylesheets")
    ///     .and_then(|v| v.as_array())
    ///     .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
    ///     .unwrap_or_default();
    /// ```
    pub config: &'a PluginSection,
    /// Resolved additional input files declared by the plugin.
    ///
    /// Paths are relative to the plugin's output directory (e.g., `build/html/`).
    /// The CLI copies each declared input from the project root to the output directory
    /// before calling `compile()`.
    pub assets: &'a HashMap<&'static str, Asset>,
}

impl<'a> PluginContext<'a> {
    pub fn compile(&'a self, plugin: &(impl FormatPlugin + ?Sized)) -> Result<()> {
        match plugin.compilation_target() {
            CompilationTarget::Pdf => self.compile_to_pdf(plugin),
            CompilationTarget::Html => self.compile_to_html(plugin),
        }
    }

    pub fn compile_to_html_string(&'a self) -> Result<String> {
        let world = self.options.world.as_ref().ok_or_else(|| {
            RheoError::project_config(
                "HTML per-file compile requires a world; this is a rheo bug (internal invariant violation)",
            )
        })?;

        let document = world.compile_html()?;

        debug!(output = %self.options.output.display(), "exporting to HTML");
        compile_document_to_string(&document)
    }

    pub fn compile_to_html(&'a self, _plugin: &(impl FormatPlugin + ?Sized)) -> Result<()> {
        let html_string = self.compile_to_html_string()?;

        debug!(size = html_string.len(), "writing HTML file");
        std::fs::write(&self.options.output, &html_string).map_err(|e| {
            RheoError::io(e, format!("writing HTML file to {:?}", self.options.output))
        })?;

        Ok(())
    }

    /// Compile each spine file to an HTML document independently.
    ///
    /// Builds transformed sources via `BuiltSpine::build()` (merge=false), then compiles
    /// each one through `RheoWorld::compile_html_file()`. Returns `(original_path, HtmlDocument)`
    /// pairs in spine order.
    pub fn compile_spine_items_to_html(
        &self,
        plugin: &(impl FormatPlugin + ?Sized),
    ) -> Result<Vec<(PathBuf, HtmlDocument)>> {
        let rheo_spine = BuiltSpine::build(
            &self.options.root,
            Some(self.spine),
            plugin.extension(),
            false,
        )?;

        let spine_paths = self.spine.generate(&self.options.root)?;

        let plugin_library = plugin.typst_library().map(|s| s.to_string());

        spine_paths
            .iter()
            .zip(rheo_spine.source.iter())
            .map(|(path, transformed_source)| {
                let mut temp_file = NamedTempFile::new_in(&self.options.root)
                    .map_err(|e| RheoError::io(e, "creating temp file for spine item HTML"))?;
                temp_file
                    .write_all(transformed_source.as_bytes())
                    .map_err(|e| {
                        RheoError::io(e, "writing transformed source to temp file")
                    })?;
                temp_file.flush().map_err(|e| {
                    RheoError::io(e, "flushing temp file")
                })?;

                let temp_path = temp_file.path();
                debug!(temp_path = %temp_path.display(), original = %path.display(), "compiling spine item to HTML");

                let document = RheoWorld::compile_html_file(
                    &self.options.root,
                    temp_path,
                    plugin.name(),
                    plugin_library.clone(),
                )?;
                Ok((path.clone(), document))
            })
            .collect()
    }

    /// Compile to PDF using the full context. By modifying fields in the PluginContext before
    /// calling this, you can modify the default PDF behaviour.
    pub fn compile_to_pdf(&'a self, plugin: &(impl FormatPlugin + ?Sized)) -> Result<()> {
        if self.spine.merge {
            // TODO: make this a `build()` function on SpineOptions
            // Build RheoSpine with AST-transformed sources (links → labels, metadata headings injected)
            let rheo_spine = BuiltSpine::build(
                &self.options.root,
                Some(self.spine),
                plugin.extension(),
                self.spine.merge,
            )?;

            debug!(file_count = rheo_spine.source.len(), "built PDF spine");

            let concatenated_source = rheo_spine.source.first().ok_or_else(|| {
                RheoError::project_config("merged PDF spine produced no source files")
            })?;
            debug!(
                source_length = concatenated_source.len(),
                "concatenated sources"
            );

            // Create temporary file with concatenated source in root directory
            let mut temp_file = NamedTempFile::new_in(&self.options.root)
                .map_err(|e| RheoError::io(e, "creating temporary file for merged PDF"))?;
            temp_file
                .write_all(concatenated_source.as_bytes())
                .map_err(|e| RheoError::io(e, "writing concatenated source to temporary file"))?;
            temp_file
                .flush()
                .map_err(|e| RheoError::io(e, "flushing temporary file"))?;

            let output_path = &self.options.output;
            let temp_path = temp_file.path();
            debug!(temp_path = %temp_path.display(), "created temporary file");

            // output_format=None because links already transformed to labels by RheoSpine
            let plugin_library = plugin.typst_library().map(|s| s.to_string());
            let document =
                RheoWorld::compile_pdf_file(&self.options.root, temp_path, None, plugin_library)?;

            debug!(output = %output_path.display(), "exporting to PDF");
            let pdf_bytes = document_to_pdf_bytes(&document)?;

            debug!(size = pdf_bytes.len(), "writing PDF file");
            std::fs::write(output_path, &pdf_bytes)
                .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output_path)))?;

            info!(output = %output_path.display(), "successfully compiled merged PDF");
            Ok(())
        } else {
            let world = self.options.world.as_ref().ok_or_else(|| {
                RheoError::project_config(
                    "PDF per-file compile requires a world; this is a rheo bug (internal invariant violation)",
                )
            })?;

            let document = world.compile_pdf()?;
            let output = &self.options.output;

            debug!(output = %output.display(), "exporting to PDF");
            let pdf_bytes = document_to_pdf_bytes(&document)?;

            debug!(size = pdf_bytes.len(), "writing PDF file");
            std::fs::write(output, &pdf_bytes)
                .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

            info!(output = %output.display(), "successfully compiled to PDF");
            Ok(())
        }
    }
}

/// The low-level compilation target for a format plugin.
pub enum CompilationTarget {
    /// Compile to an HTML document.
    Html,
    /// Compile to a paged (PDF) document.
    Pdf,
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
/// 4. **Compile**: `compile()` is called once per file (per-file mode) or once (merged mode)
/// 5. **Open**: `open()` is called if `--open` flag was passed
pub trait FormatPlugin: Send + Sync {
    /// Plugin identifier, CLI flag, and output subdirectory name.
    ///
    /// The return value serves triple duty:
    ///
    /// 1. **CLI flag**: `--<name>` enables this format (e.g., `--html`)
    /// 2. **Output subdirectory**: files are written to `build/<name>/` (e.g., `build/html/`)
    /// 3. **Format name**: passed to `RheoWorld` for link transformation and `target()` polyfill injection
    ///
    /// **Requirements:**
    /// - Must be stable (do not change between versions)
    /// - Must be lowercase
    /// - Must be alphanumeric (underscores allowed)
    /// - Should be short and descriptive
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn name(&self) -> &'static str {
    ///     "html"  // CLI: --html, output: build/html/
    /// }
    /// ```
    fn name(&self) -> &'static str;

    /// The extension that shuold be used when transforming links, if it differs from the name.
    fn extension(&self) -> &'static str {
        self.name()
    }

    /// The compilation target used by `PluginContext::compile()`.
    ///
    /// Override this if your plugin's extension differs from its compilation target.
    /// Default: "pdf" extension -> Pdf; everything else -> Html.
    fn compilation_target(&self) -> CompilationTarget {
        if self.extension() == "pdf" {
            CompilationTarget::Pdf
        } else {
            CompilationTarget::Html
        }
    }

    /// Whether this plugin merges files by default.
    ///
    /// Override to return `true` for formats that always produce a single merged output
    /// (e.g., EPUB). When `true`, the plugin is called once with all files concatenated;
    /// when `false`, the plugin is called once per file.
    ///
    /// This default can be overridden in rheo.toml via `spine.merge = true`.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn default_merge(&self) -> bool {
    ///     true  // EPUB always merges into a single file
    /// }
    /// ```
    fn default_merge(&self) -> bool {
        false
    }

    /// Set plugin-specific smart defaults when no rheo.toml section exists.
    ///
    /// Called by the CLI after loading a project when the plugin's section is not
    /// present in rheo.toml. This allows plugins to infer reasonable defaults (e.g.,
    /// inferring a title from the project name, setting default stylesheets).
    ///
    /// The `section` argument is a fresh `PluginSection` with default values; modify
    /// it in place to apply your plugin's defaults.
    ///
    /// # Arguments
    ///
    /// * `section` - The plugin's section (mutable, modify in place)
    /// * `project_name` - Derived project/file name for title inference
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn apply_defaults(&self, section: &mut PluginSection, project_name: &str) {
    ///     let spine = section.spine.get_or_insert_with(|| UniversalSpine {
    ///         title: None,
    ///         vertebrae: vec![],
    ///         merge: None,
    ///     });
    ///     if spine.title.is_none() {
    ///         spine.title = Some(DocumentTitle::to_readable_name(project_name));
    ///     }
    /// }
    /// ```
    fn apply_defaults(&self, _section: &mut PluginSection, _project_name: &str) {}

    /// Open the output for this format in the appropriate viewer.
    ///
    /// Called when the user passes `--open` with `rheo watch`. The plugin can:
    ///
    /// - Start a development server and return `OpenHandle::Server(handle)` — the CLI
    ///   calls `handle.reload()` after each successful recompile
    /// - Open files directly with the system handler and return `OpenHandle::Direct`
    /// - Return `OpenHandle::None` if preview is not supported
    ///
    /// # Arguments
    ///
    /// * `output_dir` - Path to the format's output directory (e.g., `build/html/`)
    /// * `format_name` - The format name (same as `name()`, provided for convenience)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn open(&self, output_dir: &Path, _format_name: &str) -> Result<OpenHandle> {
    ///     let runtime = tokio::runtime::Runtime::new()?;
    ///     let (server_task, reload_tx, url) = runtime.block_on(async {
    ///         start_server(output_dir.to_path_buf(), 3000).await
    ///     })?;
    ///     // ... browser opening logic ...
    ///     let handle = HtmlServerHandle { runtime, server_task, url, reload_callback };
    ///     Ok(OpenHandle::Server(Box::new(handle)))
    /// }
    /// ```
    fn open(&self, output_dir: &Path, _format_name: &str) -> crate::Result<OpenHandle> {
        open_all_files_in_folder(output_dir.to_path_buf(), self.name())?;
        Ok(OpenHandle::Direct)
    }

    /// Declare additional non-Typst input files this plugin needs.
    ///
    /// Returns a list of input files that should be copied from the project root to
    /// the plugin's output directory before compilation. This is useful for assets like
    /// stylesheets, fonts, or images.
    ///
    /// # Return value
    ///
    /// A vector of `PluginInput` declarations. Each declares:
    /// - `name`: Key to retrieve the file from `PluginContext::inputs`
    /// - `path`: Path relative to `ProjectConfig::root` where the file is expected
    /// - `required`: If `true`, missing files cause compilation errors; if `false`,
    ///   they are simply omitted from `ctx.inputs`
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn assets(&self) -> Vec<PluginInput> {
    ///     vec![
    ///         PluginInput {
    ///             name: "stylesheet",
    ///             path: "styles/main.css",
    ///             required: false,  // Optional — use default if missing
    ///         },
    ///         PluginInput {
    ///             name: "logo",
    ///             path: "assets/logo.png",
    ///             required: true,   // Required — error if missing
    ///         },
    ///     ]
    /// }
    /// ```
    ///
    /// # Reading inputs in compile()
    ///
    /// ```ignore
    /// fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
    ///     if let Some(stylesheet_path) = ctx.assets.get("stylesheet") {
    ///         let css = std::fs::read_to_string(stylesheet_path)?;
    ///         // ... use css ...
    ///     }
    ///     Ok(())
    /// }
    /// ```
    fn assets(&self) -> Vec<AssetConfig> {
        vec![]
    }

    /// Provide template files for `rheo init` to write to new projects.
    ///
    /// This method allows plugins to contribute format-specific template files
    /// (e.g., CSS for HTML, custom Typst includes) when initializing a new rheo project.
    ///
    /// # Return value
    ///
    /// A vector of `(relative_path, content)` tuples where:
    /// - `relative_path` is the file path relative to the project root (e.g., `"style.css"`, `"content/example.typ"`)
    /// - `content` is the file contents as a static string
    ///
    /// # Path conflicts
    ///
    /// If two plugins claim the same `relative_path`, rheo returns an error at init time.
    /// Core templates take precedence over plugin templates (plugins can override core paths
    /// only if the core explicitly provides empty placeholders).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn init_templates(&self) -> Vec<(&'static str, &'static str)> {
    ///     vec![
    ///         ("style.css", include_str!("templates/style.css")),
    ///         ("content/html-example.typ", include_str!("templates/example.typ")),
    ///     ]
    /// }
    /// ```
    ///
    /// # Default implementation
    ///
    /// Returns an empty vector (no template files contributed).
    fn init_templates(&self) -> Vec<(&'static str, &'static str)> {
        vec![]
    }

    /// Provide a `rheo.toml` configuration section template for `rheo init`.
    ///
    /// The returned content should use section-relative headers (e.g. `[spine]`
    /// rather than `[html.spine]`) — the `init` command automatically prefixes
    /// each header with the plugin name when building the final `rheo.toml`.
    ///
    /// # Return value
    ///
    /// - `Some(content)` — TOML snippet to embed under `[<plugin_name>.*]` in the
    ///   generated `rheo.toml`
    /// - `None` — no config section contributed (default)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn init_rheo_toml_section_template(&self) -> Option<&'static str> {
    ///     Some(include_str!("templates/init/rheo_section.toml"))
    /// }
    /// ```
    fn init_rheo_toml_section_template(&self) -> Option<&'static str> {
        None
    }

    /// Provide Typst library code to inject into all compiled files.
    ///
    /// This method allows plugins to contribute format-specific Typst functions,
    /// variables, and show rules that are automatically available in all user `.typ` files.
    ///
    /// # Return value
    ///
    /// - `Some(code)` — Typst code to inject (concatenated with core prelude and other plugin contributions)
    /// - `None` — no library code contributed (default)
    ///
    /// # Injection order
    ///
    /// Plugin library code is injected after the core rheo prelude but before user code:
    ///
    /// ```text
    /// 1. Target polyfill (for EPUB)
    /// 2. Core rheo.typ prelude (rheo-target(), is-rheo-*(), rheo_template)
    /// 3. Plugin library code (all plugins concatenated, sorted by plugin name)
    /// 4. User file content
    /// ```
    ///
    /// # Symbol conflicts
    ///
    /// Rheo does **not** detect symbol conflicts between plugin libraries. Plugins should use
    /// prefixed names (e.g., `pdf-lemma()`, `html-toc()`) to avoid collisions.
    ///
    /// # When to use this
    ///
    /// - **Do**: Provide format-specific show rules, helper functions, or constants
    /// - **Don't**: Duplicate core functionality (use `is-rheo-*()` helpers instead)
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn typst_library(&self) -> Option<&'static str> {
    ///     Some(#let pdf-watermark() = [CONFIDENTIAL])
    /// }
    /// ```
    ///
    /// # Default implementation
    ///
    /// Returns `None` (no library code contributed).
    /// TODO: make this nicer, i.e. as a Typst file
    fn typst_library(&self) -> Option<&'static str> {
        None
    }

    /// Compile one file (per-file mode) or merged output (merged mode).
    ///
    /// This is the core compilation method. The behavior depends on `ctx.spine.merge`:
    ///
    /// ## Per-file mode (`merge == false`)
    ///
    /// Called once per `.typ` file in the project. Use `ctx.options.world` to compile:
    ///
    /// ```ignore
    /// let world = ctx.options.world.ok_or_else(|| {
    ///     RheoError::project_config("plugin requires a world in per-file mode")
    /// })?;
    /// let result = typst::compile::<HtmlDocument>(world)?;
    /// ```
    ///
    /// ## Merged mode (`merge == true`)
    ///
    /// Called once with all files concatenated. The plugin must build its own worlds:
    ///
    /// ```ignore
    /// // ctx.options.world is None — build your own from ctx.options.root
    /// let world = RheoWorld::new(ctx.options.root, &concatenated_file, Some("pdf"))?;
    /// let result = typst::compile::<PagedDocument>(&world)?;
    /// ```
    ///
    /// # The merge ↔ world contract
    ///
    /// | Mode | `ctx.spine.merge` | `ctx.options.world` | `ctx.options.input` |
    /// |------|-------------------|---------------------|---------------------|
    /// | Per-file | `false` | `Some(world)` | `Some(path)` |
    /// | Merged | `true` | `None` | `None` |
    ///
    /// Plugins in per-file mode must use the pre-configured world; plugins in merged
    /// mode must create their own worlds using `ctx.options.root` as the content root.
    ///
    /// # Error handling
    ///
    /// Return errors as `Err(...)` — the CLI records failures and continues with other
    /// files/plugins. Do not swallow errors silently.
    ///
    /// # Arguments
    ///
    /// * `ctx` - Compilation context with project config, options, spine, inputs, etc.
    fn compile(&self, ctx: PluginContext<'_>) -> crate::Result<()>;
    // NOTE: the 'merge' attribute could be upraded to a parameter here, as this function operates
    // very differently according to whether it is true of false

    // TODO: because the case here is that compile is called for EVERY source file, we need a
    // `precompile` entrypoint that can do things like asset copying when merge is not true.
}

/// Open all files with a given extension in a folder using the OS default application.
pub(crate) fn open_all_files_in_folder(folder: PathBuf, ext: &str) -> crate::Result<()> {
    use tracing::{info, warn};
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
            warn!("Failed to open {}: {}", path.display(), e);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_rejects_spine() {
        let config = AssetConfig {
            name: "spine",
            default_path: "x.css",
            required: false,
        };
        let err = config.validate("test_plugin").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("spine"));
    }

    #[test]
    fn test_validate_rejects_assets() {
        let config = AssetConfig {
            name: "assets",
            default_path: "x.css",
            required: false,
        };
        let err = config.validate("test_plugin").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("assets"));
    }

    #[test]
    fn test_validate_accepts_valid_name() {
        let config = AssetConfig {
            name: "css_stylesheet",
            default_path: "style.css",
            required: false,
        };
        assert!(config.validate("test_plugin").is_ok());
    }

    #[test]
    fn test_validate_returns_misconfigured_plugin_variant() {
        let config = AssetConfig {
            name: "spine",
            default_path: "x.css",
            required: false,
        };
        let err = config.validate("test_plugin").unwrap_err();
        assert!(
            matches!(err, crate::RheoError::MisconfiguredPlugin { .. }),
            "expected MisconfiguredPlugin variant"
        );
    }
}

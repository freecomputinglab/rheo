use crate::compile::RheoCompileOptions;
use crate::config::PluginSection;
use crate::output::OutputConfig;
use crate::project::ProjectConfig;
use crate::reticulate::TracedSpine;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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

/// Declares an additional non-Typst input file needed from the project directory.
pub struct PluginInput {
    /// Key used to retrieve this input from PluginContext::inputs
    pub name: &'static str,
    /// Path relative to the project root where the file is expected
    pub path: String,
    /// If true, a missing file is a compile error; if false, it is absent from ctx.inputs
    pub required: bool,
}

/// Context passed to plugin.compile() for each compilation unit.
pub struct PluginContext<'a> {
    pub project: &'a ProjectConfig,
    pub output_config: &'a OutputConfig,
    pub options: RheoCompileOptions<'a>,
    /// Traced spine with documents, assets, title, and merge flag.
    pub spine: TracedSpine,
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
    pub config: PluginSection,
    /// Resolved additional input files declared by the plugin.
    ///
    /// Paths are relative to the plugin's output directory (e.g., `build/html/`).
    /// The CLI copies each declared input from the project root to the output directory
    /// before calling `compile()`.
    pub inputs: HashMap<&'static str, PathBuf>,
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

    /// Output file extension for generated files.
    ///
    /// Returns the file extension used for output files (without the leading dot).
    /// By default, this returns `self.name()`, but plugins can override this if their
    /// output files use a different extension than their format name.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn output_extension(&self) -> &str {
    ///     "xhtml"  // Produces .xhtml files even though format name is "html"
    /// }
    /// ```
    fn output_extension(&self) -> &str {
        self.name()
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

    /// Whether this plugin uses the Typst bundle API for compilation.
    ///
    /// Override to return `true` for formats that use `typst::compile::<Bundle>()`.
    /// When `true`, the CLI generates a bundle entry and injects it into the world
    /// before compilation. When `false`, the plugin uses per-file compilation.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// fn uses_bundle_api(&self) -> bool {
    ///     true  // HTML and PDF use bundle compilation
    /// }
    /// ```
    fn uses_bundle_api(&self) -> bool {
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
        crate::open_all_files_in_folder(output_dir.to_path_buf(), self.name())?;
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
    /// fn inputs(&self) -> Vec<PluginInput> {
    ///     vec![
    ///         PluginInput {
    ///             name: "stylesheet",
    ///             path: "styles/main.css".to_string(),
    ///             required: false,  // Optional — use default if missing
    ///         },
    ///         PluginInput {
    ///             name: "logo",
    ///             path: "assets/logo.png".to_string(),
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
    ///     if let Some(stylesheet_path) = ctx.inputs.get("stylesheet") {
    ///         let css = std::fs::read_to_string(stylesheet_path)?;
    ///         // ... use css ...
    ///     }
    ///     Ok(())
    /// }
    /// ```
    fn inputs(&self) -> Vec<PluginInput> {
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
    fn typst_library(&self) -> Option<&'static str> {
        None
    }

    /// Compile one file (per-file mode) or merged output (merged mode).
    ///
    /// This is the core compilation method. In bundle mode, every plugin receives:
    ///
    /// - `ctx.options.world`: &mut RheoWorld configured with the bundle entry as main
    /// - `ctx.spine`: TracedSpine with documents, assets, and merge flag
    /// - `ctx.options.root`: project root for resolving relative paths
    /// - `ctx.options.output`: output file/directory path
    ///
    /// ## HTML and PDF plugins
    ///
    /// Call `typst::compile::<Bundle>(&world)` for multi-file bundle output.
    ///
    /// ```ignore
    /// let world = ctx.options.world; // &mut RheoWorld
    /// let result = typst::compile::<Bundle>(world)?;
    /// ```
    ///
    /// ## EPUB exception
    ///
    /// EPUB is out of scope for bundle compilation (typst-bundle has no EPUB variant).
    /// The EPUB plugin ignores `ctx.options.world` entirely and creates its own
    /// per-file RheoWorld instances internally. The CLI always constructs a bundle
    /// world and passes it in `ctx.options.world` regardless of plugin type — this
    /// is cheap because world construction does not trigger compilation. EPUB simply
    /// does not call `typst::compile::<Bundle>(&world)` and ignores the field.
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
}

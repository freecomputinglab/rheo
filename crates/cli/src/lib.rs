use clap::{Arg, ArgAction, ArgMatches, Command};
use rheo_core::OpenHandle;
use rheo_core::assets::watch::{WatchEvent, watch_project};
use rheo_core::build::{Build, BuildOptions, resolve_build_dir};
use rheo_core::config::manifest_version;
use rheo_core::config::output::OutputConfig;
use rheo_core::config::project::ProjectConfig;
use rheo_core::{FormatPlugin, Result, RheoError, ServerHandle};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// Re-export logging functionality
pub use rheo_core::diagnostics::logging;

mod migrate;

/// Clap argument ids shared between the builder functions that declare an
/// `Arg` and the runner functions that later read it back out of
/// `ArgMatches`. clap only checks an id at runtime, so a typo in either half
/// of the pair is a panic ("Mismatch between definition and access of
/// ...") rather than a compile error; defining each id once here turns that
/// typo into an ordinary Rust name-resolution error instead.
///
/// Per-plugin format flags (`--html`, `--pdf`, ...) are NOT here: their ids
/// come from `plugin.name()` and stay dynamic, one per [`all_plugins`] entry.
mod arg {
    pub const PATH: &str = "path";
    pub const CONFIG: &str = "config";
    pub const BUILD_DIR: &str = "build-dir";
    pub const FONT_DIR: &str = "font-dir";
    pub const INPUT: &str = "input";
    pub const EMIT_BUNDLE_SOURCE: &str = "emit-bundle-source";
    pub const METADATA_TWO_PASS: &str = "metadata-two-pass";
    pub const OPEN: &str = "open";
    pub const PACKAGES: &str = "packages";
    pub const QUIET: &str = "quiet";
    pub const VERBOSE: &str = "verbose";
    pub const APPLY: &str = "apply";
}

/// Initialize logging with specified verbosity
pub fn init_logging(verbose: bool, quiet: bool) -> Result<()> {
    let verbosity = if quiet {
        logging::Verbosity::Quiet
    } else if verbose {
        logging::Verbosity::Verbose
    } else {
        logging::Verbosity::Normal
    };
    logging::init(verbosity)
}

/// Returns all known format plugins. Adding a new plugin here is the only
/// change needed in `cli` to support a new output format.
fn all_plugins() -> Vec<Box<dyn FormatPlugin>> {
    vec![
        Box::new(rheo_html::HtmlPlugin),
        Box::new(rheo_pdf::PdfPlugin),
        Box::new(rheo_epub::EpubPlugin),
    ]
}

/// Build the top-level clap `Command`, adding per-plugin `--<name>` flags
/// dynamically to `compile` and `watch` subcommands.
fn build_cli(plugins: &[Box<dyn FormatPlugin>]) -> Command {
    Command::new("rheo")
        .about("A tool for flowing Typst documents into publishable outputs")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new(arg::QUIET)
                .short('q')
                .long(arg::QUIET)
                .action(ArgAction::SetTrue)
                .conflicts_with(arg::VERBOSE)
                .global(true)
                .help("Decrease output verbosity (errors only)"),
        )
        .arg(
            Arg::new(arg::VERBOSE)
                .short('v')
                .long(arg::VERBOSE)
                .action(ArgAction::SetTrue)
                .conflicts_with(arg::QUIET)
                .global(true)
                .help("Increase output verbosity (show debug information)"),
        )
        .subcommand(build_compile_command(plugins))
        .subcommand(build_watch_command(plugins))
        .subcommand(build_clean_command())
        .subcommand(build_init_command())
        .subcommand(build_migrate_command())
        .subcommand_required(true)
        .arg_required_else_help(true)
}

fn add_format_flags(mut cmd: Command, plugins: &[Box<dyn FormatPlugin>]) -> Command {
    for plugin in plugins {
        cmd = cmd.arg(
            Arg::new(plugin.name())
                .long(plugin.name())
                .action(ArgAction::SetTrue)
                .help(format!("Compile to {} only", plugin.name())),
        );
    }
    cmd
}

/// The positional path plus the config/build-dir flags both `compile` and
/// `watch` declare first. Split from [`add_build_flags`] rather than merged with
/// it so `watch`'s own `--open` keeps its place between the two groups, leaving
/// `--help` order unchanged.
fn add_common_flags(cmd: Command) -> Command {
    cmd.arg(
        Arg::new(arg::PATH)
            .required(true)
            .index(1)
            .help("Path to project directory or single .typ file"),
    )
    .arg(
        Arg::new(arg::CONFIG)
            .long(arg::CONFIG)
            .value_name("PATH")
            .help("Path to custom rheo.toml config file"),
    )
    .arg(
        Arg::new(arg::BUILD_DIR)
            .long(arg::BUILD_DIR)
            .help("Build output directory (overrides rheo.toml if set)"),
    )
}

/// The build flags both `compile` and `watch` declare last.
fn add_build_flags(cmd: Command) -> Command {
    cmd.arg(
        Arg::new(arg::FONT_DIR)
            .long(arg::FONT_DIR)
            .value_name("DIR")
            .action(ArgAction::Append)
            .help("Additional font directory (can be repeated; appended to autoscan/config)"),
    )
    .arg(
        Arg::new(arg::EMIT_BUNDLE_SOURCE)
            .long(arg::EMIT_BUNDLE_SOURCE)
            .action(ArgAction::SetTrue)
            .help("Write each plugin's synthesized bundle source to <build_dir>/<plugin>/.rheo-bundle.typ (debug artifact, not an input)"),
    )
    .arg(
        Arg::new(arg::METADATA_TWO_PASS)
            .long(arg::METADATA_TWO_PASS)
            .action(ArgAction::SetTrue)
            .help("Recompile once more (only if needed) to resolve a #set document(title:) set inside a bounded code block for cross-vertebra metadata-of/@handle reads"),
    )
    // Declared HERE rather than on either subcommand, so `compile` and `watch`
    // get it from one definition and cannot drift. Repeatable, like `--font-dir`
    // just above.
    .arg(
        Arg::new(arg::INPUT)
            .long(arg::INPUT)
            .value_name("KEY=VALUE")
            .action(ArgAction::Append)
            .help("Set a sys.inputs key for the Typst compile (repeatable; values are always strings)"),
    )
}

/// Parse repeated `--input KEY=VALUE` into a map.
///
/// Typst has no environment access, so `sys.inputs` is the only channel by which
/// a build script can parameterise a compile. `@rheo/rookery`'s `exclude-tags` is
/// the first consumer: `--input rookery-exclude=private` builds a subsection of a
/// rookery without touching the project source.
///
/// SPLIT ON THE FIRST `=` ONLY, so a value may itself contain one
/// (`--input a=x=y` sets `a` to `x=y`).
///
/// An argument with no `=` is an ERROR naming it, never silently ignored: a
/// typo'd input reaching a package as an absent key is exactly the failure that
/// takes an afternoon to find.
///
/// `rheo-context` is REJECTED. It is rheo's own key, carrying the spine and the
/// output format, and a project able to overwrite it could hand every package a
/// forged spine.
fn parse_inputs(sub: &ArgMatches) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for raw in sub.get_many::<String>(arg::INPUT).into_iter().flatten() {
        let Some((key, value)) = raw.split_once('=') else {
            return Err(RheoError::ProjectConfig {
                message: format!(
                    "--input expects KEY=VALUE, got `{raw}` (no `=`). \
                     Example: --input rookery-exclude=private"
                ),
            });
        };
        if key.is_empty() {
            return Err(RheoError::ProjectConfig {
                message: format!("--input expects a non-empty key, got `{raw}`"),
            });
        }
        if key == rheo_core::world::RESERVED_INPUT_KEY {
            return Err(RheoError::ProjectConfig {
                message: format!(
                    "--input {key}=... is reserved: rheo owns the `{key}` key, which \
                     carries the spine and the output format that every package reads. \
                     Choose another name, e.g. a package-prefixed one like \
                     `rookery-exclude`."
                ),
            });
        }
        map.insert(key.to_string(), value.to_string());
    }
    Ok(map)
}

fn build_compile_command(plugins: &[Box<dyn FormatPlugin>]) -> Command {
    let cmd = add_build_flags(add_common_flags(
        Command::new("compile").about("Compile Typst documents to PDF, HTML, and/or EPUB"),
    ));
    add_format_flags(cmd, plugins)
}

fn build_watch_command(plugins: &[Box<dyn FormatPlugin>]) -> Command {
    let cmd = add_common_flags(
        Command::new("watch").about("Watch Typst documents and recompile on changes"),
    )
    .arg(
        Arg::new(arg::OPEN)
            .long(arg::OPEN)
            .action(ArgAction::SetTrue)
            .help("Open output in appropriate viewer (HTML opens in browser with live reload)"),
    );
    add_format_flags(add_build_flags(cmd), plugins)
}

fn build_clean_command() -> Command {
    Command::new("clean")
        .about("Clean build artifacts for a project")
        .arg(
            Arg::new(arg::PATH)
                .index(1)
                .default_value(".")
                .help("Path to project directory or single .typ file"),
        )
        .arg(
            Arg::new(arg::CONFIG)
                .long(arg::CONFIG)
                .value_name("PATH")
                .help("Path to custom rheo.toml config file"),
        )
        .arg(
            Arg::new(arg::BUILD_DIR)
                .long(arg::BUILD_DIR)
                .help("Build output directory to clean (overrides rheo.toml if set)"),
        )
        .arg(
            Arg::new(arg::PACKAGES)
                .long(arg::PACKAGES)
                .action(ArgAction::SetTrue)
                .help(
                    "Also delete cached repository checkouts for this project's \
                     [packages] namespaces (do not run during a build)",
                ),
        )
}

fn build_init_command() -> Command {
    Command::new("init")
        .about("Initialize a new Rheo project")
        .arg(
            Arg::new(arg::PATH)
                .required(true)
                .index(1)
                .help("Path to the new project directory"),
        )
}

fn build_migrate_command() -> Command {
    Command::new("migrate")
        .about("Migrate an older Rheo project to the latest version (experimental)")
        .arg(
            Arg::new(arg::PATH)
                .required(true)
                .index(1)
                .help("Path to project directory"),
        )
        .arg(
            Arg::new(arg::APPLY)
                .long(arg::APPLY)
                .action(ArgAction::SetTrue)
                .help("Apply migrations (default is a dry run that writes nothing)"),
        )
}

/// Extract enabled format names from arg matches (names of plugins whose flags are set).
fn enabled_formats_from_matches(
    matches: &ArgMatches,
    plugins: &[Box<dyn FormatPlugin>],
) -> Vec<String> {
    plugins
        .iter()
        .filter(|p| matches.get_flag(p.name()))
        .map(|p| p.name().to_string())
        .collect()
}

/// Nests a plugin's `options_toml` init-template snippet under `[<name>.*]` and
/// merges it into the generated `rheo.toml` document.
///
/// Parses the snippet with `toml_edit` rather than rewriting header lines as
/// text, so a header's exact source form (leading whitespace, a trailing
/// inline comment) never matters — the snippet's own comments and layout
/// still round-trip because `toml_edit` preserves them. `[spine]` becoming
/// `[<name>.spine]` and `[[items]]` becoming `[[<name>.items]]` need no
/// separate handling: both are just an `Item` (a `Table` or an
/// `ArrayOfTables`) relocated one level down, and `toml_edit` renders either
/// with the right header form on its own.
fn merge_plugin_toml(doc: &mut toml_edit::DocumentMut, name: &str, snippet: &str) -> Result<()> {
    let snippet_doc: toml_edit::DocumentMut = snippet.parse().map_err(|e| {
        RheoError::project_config(format!(
            "failed to parse `{name}` plugin's init template snippet: {e}"
        ))
    })?;

    match doc.as_table_mut().entry(name) {
        toml_edit::Entry::Occupied(mut existing) => {
            let table = existing.get_mut().as_table_mut().ok_or_else(|| {
                RheoError::project_config(format!(
                    "`{name}` is already a non-table key in the generated rheo.toml"
                ))
            })?;
            for (key, item) in snippet_doc.as_table().iter() {
                let mut item = item.clone();
                add_blank_line_before(&mut item);
                table.insert(key, item);
            }
        }
        toml_edit::Entry::Vacant(slot) => {
            // Implicit: this table exists only to carry the `<name>.` path
            // segment. The plugin never declares a bare `[<name>]` section of
            // its own, so one must not be printed either.
            let mut nested = toml_edit::Table::new();
            nested.set_implicit(true);
            for (key, item) in snippet_doc.as_table().iter() {
                let mut item = item.clone();
                add_blank_line_before(&mut item);
                nested.insert(key, item);
            }
            slot.insert(toml_edit::Item::Table(nested));
        }
    }
    Ok(())
}

/// Prepends a blank separating line to a relocated item's own header decor
/// (any comment already attached is kept, just pushed down a line), so a
/// plugin's appended `[<name>.*]` section doesn't run straight into whatever
/// precedes it. For `[[array.of.tables]]`, only the first entry carries the
/// header the reader sees first, so only it is touched.
fn add_blank_line_before(item: &mut toml_edit::Item) {
    let table = match item {
        toml_edit::Item::Table(t) => Some(t),
        toml_edit::Item::ArrayOfTables(a) => a.get_mut(0),
        _ => None,
    };
    if let Some(table) = table {
        let existing = table
            .decor()
            .prefix()
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
        table.decor_mut().set_prefix(format!("\n{existing}"));
    }
}

fn init_project(target_dir: &Path) -> Result<()> {
    if target_dir.exists() {
        return Err(RheoError::project_config(format!(
            "directory '{}' already exists",
            target_dir.display()
        )));
    }

    fs::create_dir_all(target_dir).map_err(|e| RheoError::io(e, "creating target directory"))?;

    let template =
        rheo_core::templates::RHEO_TOML.replace("{{VERSION}}", manifest_version::CURRENT);
    let mut toml_doc: toml_edit::DocumentMut = template.parse().map_err(|e| {
        RheoError::project_config(format!("failed to parse built-in rheo.toml template: {e}"))
    })?;
    let plugins = all_plugins();
    for plugin in &plugins {
        let tmpl = plugin.format_init_template();
        if let Some(section) = tmpl.options_toml {
            merge_plugin_toml(&mut toml_doc, plugin.name(), section)?;
        }
    }
    fs::write(target_dir.join("rheo.toml"), toml_doc.to_string())
        .map_err(|e| RheoError::io(e, "writing rheo.toml"))?;

    let content_dir = target_dir.join("content");
    fs::create_dir_all(&content_dir).map_err(|e| RheoError::io(e, "creating content directory"))?;

    fs::write(
        content_dir.join("index.typ"),
        rheo_core::templates::CONTENT_INDEX_TYP,
    )
    .map_err(|e| RheoError::io(e, "writing index.typ"))?;
    fs::write(
        content_dir.join("about.typ"),
        rheo_core::templates::CONTENT_ABOUT_TYP,
    )
    .map_err(|e| RheoError::io(e, "writing about.typ"))?;
    fs::write(
        content_dir.join("references.bib"),
        rheo_core::templates::CONTENT_REFERENCES_BIB,
    )
    .map_err(|e| RheoError::io(e, "writing references.bib"))?;

    let img_dir = content_dir.join("img");
    fs::create_dir_all(&img_dir).map_err(|e| RheoError::io(e, "creating img directory"))?;
    fs::write(
        img_dir.join("header.svg"),
        rheo_core::templates::CONTENT_IMG_HEADER_SVG,
    )
    .map_err(|e| RheoError::io(e, "writing header.svg"))?;

    // Collect template contributions from all plugins
    let mut plugin_templates: std::collections::HashMap<&str, (&str, &str)> =
        std::collections::HashMap::new();
    for plugin in &plugins {
        let tmpl = plugin.format_init_template();
        for (path, content) in tmpl.files {
            if let Some((existing_plugin, _)) = plugin_templates.get(path) {
                return Err(RheoError::project_config(format!(
                    "template path conflict: both '{}' and '{}' plugins want to write '{}'",
                    existing_plugin,
                    plugin.name(),
                    path
                )));
            }
            plugin_templates.insert(path, (plugin.name(), content));
        }
    }

    // Write plugin template files
    for (path, (plugin_name, content)) in plugin_templates {
        let file_path = target_dir.join(path);
        if let Some(parent_dir) = file_path.parent() {
            fs::create_dir_all(parent_dir)
                .map_err(|e| RheoError::io(e, "creating plugin template directory"))?;
        }
        fs::write(&file_path, content)
            .map_err(|e| RheoError::io(e, format!("writing plugin template file '{}'", path)))?;
        debug!(plugin = plugin_name, path = %path, "wrote plugin template file");
    }

    info!(path = %target_dir.display(), "initialized rheo project");
    Ok(())
}

/// Load the project, log a summary, and prepare a runnable [`Build`].
///
/// The CLI owns project loading and arg mapping; all orchestration lives in
/// [`rheo_core::Build`]. Takes `plugins` rather than calling [`all_plugins`]
/// itself, so a caller that already built the set for `BuildArgs` (which
/// every caller does) hands over that same one instead of a second instance.
fn prepare_build(
    path: &Path,
    config_path: Option<&Path>,
    opts: BuildOptions,
    plugins: Vec<Box<dyn FormatPlugin>>,
) -> Result<Build> {
    info!(path = %path.display(), "loading project");
    let project = ProjectConfig::from_path(path, config_path)?;
    let file_word = if project.typ_files.len() == 1 {
        "file"
    } else {
        "files"
    };
    info!(
        name = %project.name,
        files = project.typ_files.len(),
        "found {} Typst {}",
        project.typ_files.len(),
        file_word
    );

    Build::prepare(project, plugins, opts)
}

/// The positional path, optional config override, and derived [`BuildOptions`]
/// shared by `compile` and `watch`. `BuildOptions` doesn't derive `Clone` (it's
/// defined in `rheo_core`, not touched here), so a config-change rebuild
/// re-derives it via [`Self::build_options`] from these already-clonable
/// fields rather than re-parsing `ArgMatches` a second time.
struct BuildArgs {
    path: PathBuf,
    config: Option<PathBuf>,
    formats: Vec<String>,
    build_dir: Option<PathBuf>,
    font_dirs: Vec<PathBuf>,
    inputs: HashMap<String, String>,
    emit_bundle_source: bool,
    metadata_two_pass: bool,
}

impl BuildArgs {
    fn from_matches(sub: &ArgMatches, plugins: &[Box<dyn FormatPlugin>]) -> Result<Self> {
        Ok(Self {
            path: PathBuf::from(
                sub.get_one::<String>(arg::PATH)
                    .expect("clap enforces `path` is present (required or defaulted)"),
            ),
            config: sub.get_one::<String>(arg::CONFIG).map(PathBuf::from),
            formats: enabled_formats_from_matches(sub, plugins),
            build_dir: sub.get_one::<String>(arg::BUILD_DIR).map(PathBuf::from),
            font_dirs: sub
                .get_many::<String>(arg::FONT_DIR)
                .map(|vals| vals.map(PathBuf::from).collect())
                .unwrap_or_default(),
            inputs: parse_inputs(sub)?,
            emit_bundle_source: sub.get_flag(arg::EMIT_BUNDLE_SOURCE),
            metadata_two_pass: sub.get_flag(arg::METADATA_TWO_PASS),
        })
    }

    fn build_options(&self) -> BuildOptions {
        BuildOptions {
            formats: self.formats.clone(),
            build_dir: self.build_dir.clone(),
            font_dirs: self.font_dirs.clone(),
            inputs: self.inputs.clone(),
            emit_bundle_source: self.emit_bundle_source,
            metadata_two_pass: self.metadata_two_pass,
        }
    }
}

/// Main entry point using the builder-based dynamic CLI.
pub fn run() -> Result<()> {
    // Built once per invocation: `build_cli` reads it for the per-plugin
    // `--<name>` flags, and whichever subcommand actually runs takes
    // ownership of this same set below rather than building its own.
    let plugins = all_plugins();
    let cli = build_cli(&plugins);
    let matches = cli.get_matches();

    let quiet = matches.get_flag(arg::QUIET);
    let verbose = matches.get_flag(arg::VERBOSE);
    init_logging(verbose, quiet)?;

    match matches.subcommand() {
        Some(("compile", sub)) => run_compile(sub, plugins),
        Some(("watch", sub)) => run_watch(sub, plugins),
        Some(("clean", sub)) => run_clean(sub),
        Some(("migrate", sub)) => run_migrate(sub),
        Some(("init", sub)) => {
            let path = PathBuf::from(
                sub.get_one::<String>(arg::PATH)
                    .expect("clap enforces `path` is present (required or defaulted)"),
            );
            init_project(&path)
        }
        _ => unreachable!("subcommand_required enforced by clap"),
    }
}

/// Compile a fresh VirtualFs for the dev server and push it, then optionally
/// reload connected browsers. The initial `--open` push has nothing to reload
/// yet (the browser is only just being launched), so it passes `reload: false`;
/// both watch-loop arms want a reload after every successful push.
fn update_dev_server(build: &mut Build, server: &dyn ServerHandle, reload: bool) {
    match build.compile_for_watch() {
        Ok(Some(vfs)) => {
            let t = std::time::Instant::now();
            server.update_virtual_fs(vfs);
            debug!(
                ms = t.elapsed().as_millis(),
                "VirtualFs pushed to dev server"
            );
        }
        Ok(None) => {}
        Err(e) => warn!(error = %e, "VirtualFs compile failed, reload will serve stale content"),
    }
    if reload {
        server.reload();
    }
}

fn run_watch(sub: &ArgMatches, plugins: Vec<Box<dyn FormatPlugin>>) -> Result<()> {
    let args = BuildArgs::from_matches(sub, &plugins)?;
    let open = sub.get_flag(arg::OPEN);

    let mut build = prepare_build(
        &args.path,
        args.config.as_deref(),
        args.build_options(),
        plugins,
    )?;

    // Initial compilation (best-effort; watch continues on failure)
    if let Err(e) = build.run() {
        warn!(error = %e, "initial compilation failed");
    }

    // Open outputs if --open requested; resolve at most one server handle for
    // live reload (only the HTML plugin ever returns OpenHandle::Server).
    let mut server: Option<Box<dyn ServerHandle>> = None;
    if open {
        for plugin in build.plugins() {
            let out_dir = build.output_config().dir_for_plugin(plugin.name());
            match plugin.open(&out_dir, plugin.name()) {
                Ok(OpenHandle::Server(handle)) if server.is_none() => server = Some(handle),
                Ok(_) => {}
                Err(e) => warn!(error = %e, plugin = plugin.name(), "failed to open"),
            }
        }
        // Push initial in-memory VirtualFs to the HTML dev server so it serves
        // HTML from memory rather than re-reading disk on each request.
        // compile_for_watch() reuses comemo-cached Typst state from build.run(),
        // so this second compile is near-instant.
        if let Some(server) = &server {
            update_dev_server(&mut build, server.as_ref(), false);
        }
    }

    let watch_project_cfg = build.project().clone();
    let build_dir_canonical = build
        .output_config()
        .base
        .canonicalize()
        .unwrap_or_else(|_| build.output_config().base.clone());
    // Capture the asset sources to watch (declared assets, copy globs, package
    // roots) once, before `build` is moved into the change callback.
    let asset_spec = build.watch_asset_spec();

    watch_project(
        &watch_project_cfg,
        &build_dir_canonical,
        &asset_spec,
        move |event| {
            match event {
                WatchEvent::FilesChanged => {
                    info!("files changed, recompiling");
                    if build.run().is_ok()
                        && let Some(server) = &server
                    {
                        update_dev_server(&mut build, server.as_ref(), true);
                    }
                }
                WatchEvent::ConfigChanged => {
                    info!("config changed, reloading");
                    match prepare_build(
                        &args.path,
                        args.config.as_deref(),
                        args.build_options(),
                        all_plugins(),
                    ) {
                        Ok(new_build) => {
                            build = new_build;
                            if build.run().is_ok()
                                && let Some(server) = &server
                            {
                                update_dev_server(&mut build, server.as_ref(), true);
                            }
                        }
                        Err(e) => warn!(error = %e, "failed to reload config"),
                    }
                }
            }
            Ok(())
        },
    )
}

fn run_compile(sub: &ArgMatches, plugins: Vec<Box<dyn FormatPlugin>>) -> Result<()> {
    let args = BuildArgs::from_matches(sub, &plugins)?;
    let build = prepare_build(
        &args.path,
        args.config.as_deref(),
        args.build_options(),
        plugins,
    )?;
    build.run().map(|_| ())
}

fn run_clean(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(
        sub.get_one::<String>(arg::PATH)
            .expect("clap enforces `path` is present (required or defaulted)"),
    );
    let config = sub.get_one::<String>(arg::CONFIG).map(PathBuf::from);
    let build_dir = sub.get_one::<String>(arg::BUILD_DIR).map(PathBuf::from);

    info!(path = %path.display(), "loading project");
    let project = ProjectConfig::from_path(&path, config.as_deref())?;
    let resolved_build_dir = resolve_build_dir(&project, build_dir)?;
    let output_config = OutputConfig::new(&project.root, resolved_build_dir);
    info!(project = %project.name, "cleaning build artifacts");
    output_config.clean()?;
    info!(project = %project.name, "build artifacts removed");

    // Opt-in, and never on the build path: a checkout is cheap to re-clone, but
    // deleting one out from under a running build is not recoverable.
    if sub.get_flag(arg::PACKAGES) {
        let resolver = rheo_core::packages::PackageResolver::new(&project.config.packages);
        for (namespace, result) in resolver.prune_checkouts() {
            match result {
                Ok(0) => info!(namespace, "no cached checkouts to remove"),
                Ok(n) => info!(namespace, removed = n, "removed cached checkouts"),
                Err(e) => warn!(namespace, error = %e, "could not remove cached checkouts"),
            }
        }
    }
    Ok(())
}

fn run_migrate(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(
        sub.get_one::<String>(arg::PATH)
            .expect("clap enforces `path` is present (required or defaulted)"),
    );
    let apply = sub.get_flag(arg::APPLY);
    info!(path = %path.display(), apply, "migrating project");
    migrate::migrate_project(&path, apply)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_plugins_contains_three_formats() {
        let plugins = all_plugins();
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"html"));
        assert!(names.contains(&"pdf"));
        assert!(names.contains(&"epub"));
        assert!(
            names.len() >= 3,
            "Expected at least 3 plugins, got {}",
            names.len()
        );
    }

    // The old line-based `prefix_toml_headers` mishandled a header with
    // leading whitespace or a trailing inline comment (see its doc comment,
    // now gone) since it only matched a bare `[header]` line. Parsing with
    // `toml_edit` has no such restriction; these two pin the forms that used
    // to be left un-nested.

    #[test]
    fn merge_plugin_toml_nests_header_with_leading_whitespace() {
        let mut doc: toml_edit::DocumentMut = "version = \"0.1.0\"\n".parse().unwrap();
        merge_plugin_toml(&mut doc, "pdf", "  [spine]\n  title = \"rheo_project\"\n").unwrap();
        assert_eq!(doc["pdf"]["spine"]["title"].as_str(), Some("rheo_project"));
        assert!(doc.to_string().contains("[pdf.spine]"));
    }

    #[test]
    fn merge_plugin_toml_nests_header_with_trailing_comment() {
        let mut doc: toml_edit::DocumentMut = "version = \"0.1.0\"\n".parse().unwrap();
        merge_plugin_toml(
            &mut doc,
            "epub",
            "[spine] # per-plugin override\ntitle = \"rheo_project\"\n",
        )
        .unwrap();
        assert_eq!(doc["epub"]["spine"]["title"].as_str(), Some("rheo_project"));
        assert!(doc.to_string().contains("[epub.spine]"));
    }

    #[test]
    fn merge_plugin_toml_preserves_surrounding_comments() {
        let mut doc: toml_edit::DocumentMut = "# keep me\nversion = \"0.1.0\"\n".parse().unwrap();
        merge_plugin_toml(&mut doc, "pdf", "[spine]\ntitle = \"rheo_project\"\n").unwrap();
        assert!(doc.to_string().starts_with("# keep me"));
    }

    /// Nesting now happens structurally (a fresh `[<name>]` table keyed by
    /// the plugin's own name), not by text-matching an already-prefixed
    /// header, so a repeat merge can't double-nest — it's a no-op rewrite of
    /// the same values rather than needing a dedicated idempotence check.
    #[test]
    fn merge_plugin_toml_repeat_call_is_a_no_op() {
        let mut doc: toml_edit::DocumentMut = "version = \"0.1.0\"\n".parse().unwrap();
        let snippet = "[spine]\ntitle = \"rheo_project\"\n";
        merge_plugin_toml(&mut doc, "pdf", snippet).unwrap();
        merge_plugin_toml(&mut doc, "pdf", snippet).unwrap();
        assert_eq!(doc["pdf"]["spine"]["title"].as_str(), Some("rheo_project"));
        assert_eq!(doc.to_string().matches("[pdf.spine]").count(), 1);
    }
}

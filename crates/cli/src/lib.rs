use clap::{Arg, ArgAction, ArgMatches, Command};
use rheo_core::OpenHandle;
use rheo_core::build::{Build, BuildOptions, resolve_build_dir};
use rheo_core::manifest_version;
use rheo_core::output::OutputConfig;
use rheo_core::project::ProjectConfig;
use rheo_core::watch::{WatchEvent, watch_project};
use rheo_core::{FormatPlugin, Result, RheoError};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// Re-export logging functionality
pub use rheo_core::logging;

mod migrate;

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
fn build_cli() -> Command {
    let plugins = all_plugins();
    Command::new("rheo")
        .about("A tool for flowing Typst documents into publishable outputs")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .action(ArgAction::SetTrue)
                .conflicts_with("verbose")
                .global(true)
                .help("Decrease output verbosity (errors only)"),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .action(ArgAction::SetTrue)
                .conflicts_with("quiet")
                .global(true)
                .help("Increase output verbosity (show debug information)"),
        )
        .subcommand(build_compile_command(&plugins))
        .subcommand(build_watch_command(&plugins))
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

fn build_compile_command(plugins: &[Box<dyn FormatPlugin>]) -> Command {
    let cmd = Command::new("compile")
        .about("Compile Typst documents to PDF, HTML, and/or EPUB")
        .arg(
            Arg::new("path")
                .required(true)
                .index(1)
                .help("Path to project directory or single .typ file"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .value_name("PATH")
                .help("Path to custom rheo.toml config file"),
        )
        .arg(
            Arg::new("build-dir")
                .long("build-dir")
                .help("Build output directory (overrides rheo.toml if set)"),
        )
        .arg(
            Arg::new("font-dir")
                .long("font-dir")
                .value_name("DIR")
                .action(ArgAction::Append)
                .help("Additional font directory (can be repeated; appended to autoscan/config)"),
        );
    add_format_flags(cmd, plugins)
}

fn build_watch_command(plugins: &[Box<dyn FormatPlugin>]) -> Command {
    let cmd = Command::new("watch")
        .about("Watch Typst documents and recompile on changes")
        .arg(
            Arg::new("path")
                .required(true)
                .index(1)
                .help("Path to project directory or single .typ file"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .value_name("PATH")
                .help("Path to custom rheo.toml config file"),
        )
        .arg(
            Arg::new("build-dir")
                .long("build-dir")
                .help("Build output directory (overrides rheo.toml if set)"),
        )
        .arg(
            Arg::new("open")
                .long("open")
                .action(ArgAction::SetTrue)
                .help("Open output in appropriate viewer (HTML opens in browser with live reload)"),
        )
        .arg(
            Arg::new("font-dir")
                .long("font-dir")
                .value_name("DIR")
                .action(ArgAction::Append)
                .help("Additional font directory (can be repeated; appended to autoscan/config)"),
        );
    add_format_flags(cmd, plugins)
}

fn build_clean_command() -> Command {
    Command::new("clean")
        .about("Clean build artifacts for a project")
        .arg(
            Arg::new("path")
                .index(1)
                .default_value(".")
                .help("Path to project directory or single .typ file"),
        )
        .arg(
            Arg::new("config")
                .long("config")
                .value_name("PATH")
                .help("Path to custom rheo.toml config file"),
        )
        .arg(
            Arg::new("build-dir")
                .long("build-dir")
                .help("Build output directory to clean (overrides rheo.toml if set)"),
        )
}

fn build_init_command() -> Command {
    Command::new("init")
        .about("Initialize a new Rheo project")
        .arg(
            Arg::new("path")
                .required(true)
                .index(1)
                .help("Path to the new project directory"),
        )
}

fn build_migrate_command() -> Command {
    Command::new("migrate")
        .about("Migrate an older Rheo project to the latest version (experimental)")
        .arg(
            Arg::new("path")
                .required(true)
                .index(1)
                .help("Path to project directory"),
        )
        .arg(
            Arg::new("apply")
                .long("apply")
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

/// Rewrites TOML section headers to be nested under a given prefix.
///
/// `[spine]` becomes `[prefix.spine]` and `[[items]]` becomes `[[prefix.items]]`.
/// Only matches bare headers (no leading whitespace, no inline comments).
/// Non-header lines and already-prefixed headers are returned unchanged.
fn prefix_toml_headers(content: &str, prefix: &str) -> String {
    content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            // Skip already-prefixed headers (idempotent)
            if trimmed.starts_with(&format!("[{prefix}."))
                || trimmed.starts_with(&format!("[[{prefix}."))
            {
                return line.to_string();
            }
            if let Some(inner) = trimmed
                .strip_prefix("[[")
                .and_then(|s| s.strip_suffix("]]"))
            {
                let inner = inner.trim();
                if !inner.is_empty()
                    && inner
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
                {
                    return line.replace(trimmed, &format!("[[{prefix}.{inner}]]"));
                }
            } else if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
            {
                let inner = inner.trim();
                if !inner.is_empty()
                    && inner
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
                {
                    return line.replace(trimmed, &format!("[{prefix}.{inner}]"));
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn init_project(target_dir: &Path) -> Result<()> {
    if target_dir.exists() {
        return Err(RheoError::project_config(format!(
            "directory '{}' already exists",
            target_dir.display()
        )));
    }

    fs::create_dir_all(target_dir).map_err(|e| RheoError::io(e, "creating target directory"))?;

    let mut toml_content =
        rheo_core::init_templates::RHEO_TOML.replace("{{VERSION}}", manifest_version::CURRENT);
    for plugin in all_plugins() {
        let tmpl = plugin.format_init_template();
        if let Some(section) = tmpl.options_toml {
            toml_content.push('\n');
            toml_content.push_str(&prefix_toml_headers(section, plugin.name()));
            toml_content.push('\n');
        }
    }
    fs::write(target_dir.join("rheo.toml"), &toml_content)
        .map_err(|e| RheoError::io(e, "writing rheo.toml"))?;

    let content_dir = target_dir.join("content");
    fs::create_dir_all(&content_dir).map_err(|e| RheoError::io(e, "creating content directory"))?;

    fs::write(
        content_dir.join("index.typ"),
        rheo_core::init_templates::CONTENT_INDEX_TYP,
    )
    .map_err(|e| RheoError::io(e, "writing index.typ"))?;
    fs::write(
        content_dir.join("about.typ"),
        rheo_core::init_templates::CONTENT_ABOUT_TYP,
    )
    .map_err(|e| RheoError::io(e, "writing about.typ"))?;
    fs::write(
        content_dir.join("references.bib"),
        rheo_core::init_templates::CONTENT_REFERENCES_BIB,
    )
    .map_err(|e| RheoError::io(e, "writing references.bib"))?;

    let img_dir = content_dir.join("img");
    fs::create_dir_all(&img_dir).map_err(|e| RheoError::io(e, "creating img directory"))?;
    fs::write(
        img_dir.join("header.svg"),
        rheo_core::init_templates::CONTENT_IMG_HEADER_SVG,
    )
    .map_err(|e| RheoError::io(e, "writing header.svg"))?;

    // Collect template contributions from all plugins
    let mut plugin_templates: std::collections::HashMap<&str, (&str, &str)> =
        std::collections::HashMap::new();
    for plugin in all_plugins() {
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
/// [`rheo_core::Build`].
fn prepare_build(
    path: &Path,
    config_path: Option<&Path>,
    build_dir: Option<PathBuf>,
    enabled_from_cli: Vec<String>,
    cli_font_dirs: Vec<PathBuf>,
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

    let opts = BuildOptions {
        formats: enabled_from_cli,
        build_dir,
        font_dirs: cli_font_dirs,
    };
    Build::prepare(project, all_plugins(), opts)
}

/// Main entry point using the builder-based dynamic CLI.
pub fn run() -> Result<()> {
    let cli = build_cli();
    let matches = cli.get_matches();

    let quiet = matches.get_flag("quiet");
    let verbose = matches.get_flag("verbose");
    init_logging(verbose, quiet)?;

    match matches.subcommand() {
        Some(("compile", sub)) => run_compile(sub),
        Some(("watch", sub)) => run_watch(sub),
        Some(("clean", sub)) => run_clean(sub),
        Some(("migrate", sub)) => run_migrate(sub),
        Some(("init", sub)) => {
            let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
            init_project(&path)
        }
        _ => unreachable!("subcommand_required enforced by clap"),
    }
}

fn run_watch(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
    let config_path = sub.get_one::<String>("config").map(PathBuf::from);
    let build_dir = sub.get_one::<String>("build-dir").map(PathBuf::from);
    let cli_font_dirs: Vec<PathBuf> = sub
        .get_many::<String>("font-dir")
        .map(|vals| vals.map(PathBuf::from).collect())
        .unwrap_or_default();
    let open = sub.get_flag("open");

    let all = all_plugins();
    let enabled = enabled_formats_from_matches(sub, &all);

    let mut build = prepare_build(
        &path,
        config_path.as_deref(),
        build_dir.clone(),
        enabled.clone(),
        cli_font_dirs.clone(),
    )?;

    // Initial compilation (best-effort; watch continues on failure)
    if let Err(e) = build.run() {
        warn!(error = %e, "initial compilation failed");
    }

    // Open outputs if --open requested; collect server handles for live reload
    let mut open_handles: Vec<OpenHandle> = Vec::new();
    if open {
        for plugin in build.plugins() {
            let out_dir = build.output_config().dir_for_plugin(plugin.name());
            match plugin.open(&out_dir, plugin.name()) {
                Ok(handle) => open_handles.push(handle),
                Err(e) => warn!(error = %e, plugin = plugin.name(), "failed to open"),
            }
        }
        // Push initial in-memory VirtualFs to the HTML dev server so it serves
        // HTML from memory rather than re-reading disk on each request.
        // compile_for_watch() reuses comemo-cached Typst state from build.run(),
        // so this second compile is near-instant.
        if let Some(OpenHandle::Server(server)) = open_handles
            .iter()
            .find(|h| matches!(h, OpenHandle::Server(_)))
        {
            match build.compile_for_watch() {
                Ok(Some(vfs)) => {
                    server.update_virtual_fs(vfs);
                    debug!("initial VirtualFs pushed to dev server");
                }
                Ok(None) => {}
                Err(e) => warn!(error = %e, "initial VirtualFs compile failed, serving from disk"),
            }
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
                    if build.run().is_ok() {
                        // Update HTML dev server VirtualFs before triggering browser reload.
                        for handle in &open_handles {
                            if let OpenHandle::Server(server) = handle {
                                match build.compile_for_watch() {
                                    Ok(Some(vfs)) => {
                                        let t = std::time::Instant::now();
                                        server.update_virtual_fs(vfs);
                                        debug!(ms = t.elapsed().as_millis(), "VirtualFs updated");
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        warn!(error = %e, "VirtualFs compile failed, reload will serve stale content")
                                    }
                                }
                                server.reload();
                                break; // only one server handle expected
                            }
                        }
                    }
                }
                WatchEvent::ConfigChanged => {
                    info!("config changed, reloading");
                    match prepare_build(
                        &path,
                        config_path.as_deref(),
                        build_dir.clone(),
                        enabled.clone(),
                        cli_font_dirs.clone(),
                    ) {
                        Ok(new_build) => {
                            build = new_build;
                            if build.run().is_ok() {
                                for handle in &open_handles {
                                    if let OpenHandle::Server(server) = handle {
                                        match build.compile_for_watch() {
                                            Ok(Some(vfs)) => server.update_virtual_fs(vfs),
                                            Ok(None) => {}
                                            Err(e) => {
                                                warn!(error = %e, "VirtualFs compile failed after config reload")
                                            }
                                        }
                                        server.reload();
                                        break;
                                    }
                                }
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

fn run_compile(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
    let config = sub.get_one::<String>("config").map(PathBuf::from);
    let build_dir = sub.get_one::<String>("build-dir").map(PathBuf::from);
    let cli_font_dirs: Vec<PathBuf> = sub
        .get_many::<String>("font-dir")
        .map(|vals| vals.map(PathBuf::from).collect())
        .unwrap_or_default();

    let all = all_plugins();
    let enabled = enabled_formats_from_matches(sub, &all);

    let mut build = prepare_build(&path, config.as_deref(), build_dir, enabled, cli_font_dirs)?;
    build.run().map(|_| ())
}

fn run_clean(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
    let config = sub.get_one::<String>("config").map(PathBuf::from);
    let build_dir = sub.get_one::<String>("build-dir").map(PathBuf::from);

    info!(path = %path.display(), "loading project");
    let project = ProjectConfig::from_path(&path, config.as_deref())?;
    let resolved_build_dir = resolve_build_dir(&project, build_dir)?;
    let output_config = OutputConfig::new(&project.root, resolved_build_dir);
    info!(project = %project.name, "cleaning build artifacts");
    output_config.clean()?;
    info!(project = %project.name, "build artifacts removed");
    Ok(())
}

fn run_migrate(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
    let apply = sub.get_flag("apply");
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
}

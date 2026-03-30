use clap::{Arg, ArgAction, ArgMatches, Command};
use rheo_core::FormatPlugin;

pub(crate) fn build_cli(plugins: &[Box<dyn FormatPlugin>]) -> Command {
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
        .subcommand(build_compile_command(plugins))
        .subcommand(build_watch_command(plugins))
        .subcommand(build_clean_command())
        .subcommand(build_init_command())
        .subcommand_required(true)
        .arg_required_else_help(true)
}

pub(crate) fn add_format_flags(mut cmd: Command, plugins: &[Box<dyn FormatPlugin>]) -> Command {
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

pub(crate) fn build_compile_command(plugins: &[Box<dyn FormatPlugin>]) -> Command {
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
        );
    add_format_flags(cmd, plugins)
}

pub(crate) fn build_watch_command(plugins: &[Box<dyn FormatPlugin>]) -> Command {
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
        );
    add_format_flags(cmd, plugins)
}

pub(crate) fn build_clean_command() -> Command {
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

pub(crate) fn build_init_command() -> Command {
    Command::new("init")
        .about("Initialize a new Rheo project")
        .arg(
            Arg::new("path")
                .required(true)
                .index(1)
                .help("Path to the new project directory"),
        )
}

/// Extract enabled format names from arg matches (names of plugins whose flags are set).
pub(crate) fn enabled_formats_from_matches(
    matches: &ArgMatches,
    plugins: &[Box<dyn FormatPlugin>],
) -> Vec<String> {
    plugins
        .iter()
        .filter(|p| matches.get_flag(p.name()))
        .map(|p| p.name().to_string())
        .collect()
}

/// Determine which format names to compile based on CLI flags and config defaults.
///
/// Priority:
/// 1. CLI flags (any set → use only those)
/// 2. Config `formats` list (non-empty → use that)
/// 3. All plugins (fallback)
pub(crate) fn determine_formats(
    enabled_from_cli: Vec<String>,
    config_defaults: &[String],
    all: &[Box<dyn FormatPlugin>],
) -> Vec<String> {
    if !enabled_from_cli.is_empty() {
        return enabled_from_cli;
    }
    if !config_defaults.is_empty() {
        return config_defaults.to_vec();
    }
    all.iter().map(|p| p.name().to_string()).collect()
}

/// Filter `all_plugins()` to only those whose names appear in `formats`.
pub(crate) fn plugins_for_formats(
    formats: &[String],
    all: Vec<Box<dyn FormatPlugin>>,
) -> Vec<Box<dyn FormatPlugin>> {
    all.into_iter()
        .filter(|p| formats.iter().any(|f| f == p.name()))
        .collect()
}

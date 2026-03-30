pub mod args;
pub mod init;
pub mod orchestrate;

use clap::ArgMatches;
use rheo_core::watch::{WatchEvent, watch_project};
use rheo_core::{FormatPlugin, OpenHandle, Result};
use std::path::PathBuf;
use tracing::info;

// Re-export logging functionality
pub use rheo_core::logging;

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
pub fn all_plugins() -> Vec<Box<dyn FormatPlugin>> {
    vec![
        Box::new(rheo_html::HtmlPlugin),
        Box::new(rheo_pdf::PdfPlugin),
        Box::new(rheo_epub::EpubPlugin),
    ]
}

/// Main entry point using the builder-based dynamic CLI.
pub fn run() -> Result<()> {
    let plugins = all_plugins();
    let cli = args::build_cli(&plugins);
    let matches = cli.get_matches();

    let quiet = matches.get_flag("quiet");
    let verbose = matches.get_flag("verbose");
    init_logging(verbose, quiet)?;

    match matches.subcommand() {
        Some(("compile", sub)) => run_compile(sub),
        Some(("watch", sub)) => run_watch(sub),
        Some(("clean", sub)) => run_clean(sub),
        Some(("init", sub)) => {
            let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
            init::init_project(&path, all_plugins)
        }
        _ => unreachable!("subcommand_required enforced by clap"),
    }
}

fn run_watch(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
    let config_path = sub.get_one::<String>("config").map(PathBuf::from);
    let build_dir = sub.get_one::<String>("build-dir").map(PathBuf::from);
    let open = sub.get_flag("open");

    let all = all_plugins();
    let enabled = args::enabled_formats_from_matches(sub, &all);

    let mut ctx = orchestrate::setup_compilation_context(
        &path,
        config_path.as_deref(),
        build_dir.clone(),
        enabled.clone(),
        all_plugins,
    )?;

    // Initial compilation (best-effort; watch continues on failure)
    if let Err(e) = orchestrate::perform_compilation(&ctx.project, &ctx.output_config, &ctx.plugins)
    {
        tracing::warn!(error = %e, "initial compilation failed");
    }

    // Open outputs if --open requested; collect server handles for live reload
    let mut open_handles: Vec<OpenHandle> = Vec::new();
    if open {
        for plugin in &ctx.plugins {
            let out_dir = ctx.output_config.dir_for_plugin(plugin.name());
            match plugin.open(&out_dir, plugin.name()) {
                Ok(handle) => open_handles.push(handle),
                Err(e) => tracing::warn!(error = %e, plugin = plugin.name(), "failed to open"),
            }
        }
    }

    let watch_project_cfg = ctx.project.clone();
    let build_dir_canonical = ctx
        .output_config
        .base
        .canonicalize()
        .unwrap_or_else(|_| ctx.output_config.base.clone());

    watch_project(&watch_project_cfg, &build_dir_canonical, move |event| {
        match event {
            WatchEvent::FilesChanged => {
                info!("files changed, recompiling");
                if orchestrate::perform_compilation(
                    &ctx.project,
                    &ctx.output_config,
                    &ctx.plugins,
                )
                .is_ok()
                {
                    for handle in &open_handles {
                        if let OpenHandle::Server(server) = handle {
                            server.reload();
                        }
                    }
                }
            }
            WatchEvent::ConfigChanged => {
                info!("config changed, reloading");
                match orchestrate::setup_compilation_context(
                    &path,
                    config_path.as_deref(),
                    build_dir.clone(),
                    enabled.clone(),
                    all_plugins,
                ) {
                    Ok(new_ctx) => {
                        ctx = new_ctx;
                        if orchestrate::perform_compilation(
                            &ctx.project,
                            &ctx.output_config,
                            &ctx.plugins,
                        )
                        .is_ok()
                        {
                            for handle in &open_handles {
                                if let OpenHandle::Server(server) = handle {
                                    server.reload();
                                }
                            }
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "failed to reload config"),
                }
            }
        }
        Ok(())
    })
}

fn run_compile(sub: &ArgMatches) -> Result<()> {
    let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
    let config = sub.get_one::<String>("config").map(PathBuf::from);
    let build_dir = sub.get_one::<String>("build-dir").map(PathBuf::from);

    let all = all_plugins();
    let enabled = args::enabled_formats_from_matches(sub, &all);

    let ctx =
        orchestrate::setup_compilation_context(&path, config.as_deref(), build_dir, enabled, all_plugins)?;

    orchestrate::perform_compilation(&ctx.project, &ctx.output_config, &ctx.plugins)
}

fn run_clean(sub: &ArgMatches) -> Result<()> {
    use rheo_core::output::OutputConfig;
    use rheo_core::project::ProjectConfig;

    let path = PathBuf::from(sub.get_one::<String>("path").unwrap());
    let config = sub.get_one::<String>("config").map(PathBuf::from);
    let build_dir = sub.get_one::<String>("build-dir").map(PathBuf::from);

    info!(path = %path.display(), "loading project");
    let project = ProjectConfig::from_path(&path, config.as_deref())?;
    let resolved_build_dir = orchestrate::resolve_build_dir(&project, build_dir)?;
    let output_config = OutputConfig::new(&project.root, resolved_build_dir);
    info!(project = %project.name, "cleaning build artifacts");
    output_config.clean()?;
    info!(project = %project.name, "build artifacts removed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{determine_formats, plugins_for_formats};

    #[test]
    fn test_determine_formats_cli_flags_override_config() {
        let all = all_plugins();
        let config_defaults = vec!["pdf".to_string()];
        let enabled = vec!["pdf".to_string()];

        let formats = determine_formats(enabled, &config_defaults, &all);
        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&"pdf".to_string()));
    }

    #[test]
    fn test_determine_formats_uses_config_defaults_when_no_flags() {
        let all = all_plugins();
        let config_defaults = vec!["html".to_string()];
        let enabled: Vec<String> = vec![];

        let formats = determine_formats(enabled, &config_defaults, &all);
        assert_eq!(formats.len(), 1);
        assert!(formats.contains(&"html".to_string()));
    }

    #[test]
    fn test_determine_formats_falls_back_to_all_when_empty() {
        let all = all_plugins();
        let config_defaults: Vec<String> = vec![];
        let enabled: Vec<String> = vec![];

        let formats = determine_formats(enabled, &config_defaults, &all);
        assert_eq!(formats.len(), all_plugins().len());
        assert!(formats.contains(&"pdf".to_string()));
        assert!(formats.contains(&"html".to_string()));
        assert!(formats.contains(&"epub".to_string()));
    }

    #[test]
    fn test_determine_formats_multiple_cli_flags() {
        let all = all_plugins();
        let config_defaults = vec!["epub".to_string()];
        let enabled = vec!["pdf".to_string(), "html".to_string()];

        let formats = determine_formats(enabled, &config_defaults, &all);
        assert_eq!(formats.len(), 2);
        assert!(formats.contains(&"pdf".to_string()));
        assert!(formats.contains(&"html".to_string()));
    }

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

    #[test]
    fn test_plugins_for_formats() {
        let formats = vec!["pdf".to_string(), "html".to_string()];
        let plugins = plugins_for_formats(&formats, all_plugins());
        assert_eq!(plugins.len(), 2);
        let names: Vec<&str> = plugins.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"pdf"));
        assert!(names.contains(&"html"));
        assert!(!names.contains(&"epub"));
    }
}

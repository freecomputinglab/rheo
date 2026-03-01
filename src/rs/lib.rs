pub mod cli;
pub mod compile;
pub mod config;
pub mod constants;
pub mod error;
pub mod diagnostics;
pub mod init;
pub mod logging;
pub mod manifest_version;
pub mod output;
pub mod path_utils;
pub mod plugins;
pub mod project;
pub mod results;
pub mod reticulate;
pub mod server;
pub mod validation;
pub mod watch;
pub mod world;

pub use cli::Cli;
pub use config::RheoConfig;
pub use constants::*;
pub use error::RheoError;
pub use globset::{Glob, GlobSet, GlobSetBuilder};
pub use manifest_version::ManifestVersion;
pub use path_utils::PathExt;
pub use results::{CompilationResults, FormatResult};
use std::path::PathBuf;
use tracing::{info, warn};
use walkdir::WalkDir;

/// Result type alias using RheoError
pub type Result<T> = std::result::Result<T, RheoError>;

/// Internal output format enum for link transformation and world configuration.
/// Not part of the public API — plugins identify themselves by name string only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    Html,
    Epub,
    Pdf,
}

pub fn open_all_files_in_folder(folder: PathBuf, ext: &str) -> Result<()> {
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

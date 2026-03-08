pub mod compile;
pub mod config;
pub mod constants;
pub mod diagnostics;
pub mod error;
pub mod html_compile;
pub mod init_templates;
pub mod logging;
pub mod manifest_version;
pub mod output;
pub mod path_utils;
pub mod pdf_utils;
pub mod project;
pub mod results;
pub mod reticulate;
pub mod validation;
pub mod watch;
pub mod world;

// Re-export plugins module as separate file (was plugins/mod.rs)
include!("plugins.rs");

// Note: Cli is now in rheo-cli crate, not exported here
pub use config::RheoConfig;
pub use constants::*;

/// Bundled default HTML stylesheet (from `src/templates/init/style.css`).
/// HTML plugin uses this when the project doesn't provide its own style.css.
pub const DEFAULT_HTML_STYLESHEET: &str = include_str!("templates/init/style.css");
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

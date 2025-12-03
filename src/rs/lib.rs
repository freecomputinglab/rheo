pub mod cli;
pub mod compile;
pub mod config;
pub mod error;
pub mod formats;
pub mod logging;
pub mod output;
pub mod project;
pub mod server;
pub mod spine;
pub mod watch;
pub mod world;

pub use cli::Cli;
pub use config::RheoConfig;
pub use error::RheoError;
pub use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fmt;
use std::path::PathBuf;
use tracing::{info, warn};
use walkdir::WalkDir;

/// Result type alias using RheoError
pub type Result<T> = std::result::Result<T, RheoError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum OutputFormat {
    Html,
    Epub,
    Pdf,
}

impl OutputFormat {
    pub fn all_variants() -> Vec<Self> {
        vec![Self::Html, Self::Epub, Self::Pdf]
    }
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl<'de> serde::Deserialize<'de> for OutputFormat {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "html" => Ok(OutputFormat::Html),
            "epub" => Ok(OutputFormat::Epub),
            "pdf" => Ok(OutputFormat::Pdf),
            _ => Err(serde::de::Error::unknown_variant(
                &s,
                &["html", "epub", "pdf"],
            )),
        }
    }
}

pub fn open_all_files_in_folder(folder: PathBuf, fmt: OutputFormat) -> Result<()> {
    let ext = if fmt == OutputFormat::Epub {
        "epub"
    } else {
        "pdf"
    };

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

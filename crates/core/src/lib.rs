pub mod compile;
pub mod config;
pub mod constants;
pub mod diagnostics;
pub mod error;
pub mod init_templates;
pub mod logging;
pub mod manifest_version;
pub mod output;
pub mod path_utils;
pub mod pdf_utils;
pub mod plugins;
pub mod project;
pub mod results;
pub mod reticulate;
pub mod typst_types;
pub mod validation;
pub mod watch;
pub mod world;

// Note: Cli is now in rheo-cli crate, not exported here

// === Core types (already exported) ===
pub use config::RheoConfig;
pub use constants::*;
pub use error::RheoError;
pub use globset::{Glob, GlobSet, GlobSetBuilder};
pub use manifest_version::ManifestVersion;
pub use path_utils::PathExt;
pub use results::{CompilationResults, FormatResult};

// === Plugin API re-exports ===

// Compile options and context
pub use compile::RheoCompileOptions;

// Configuration types
pub use config::{PluginSection, Spine};

// Plugin trait and context
pub use plugins::{FormatPlugin, OpenHandle, PluginContext, PluginInput, ServerHandle};

// Re-export TracedSpine for use in bundle compilation
pub use reticulate::{SpineDocument, TracedSpine};

// HTML and PDF compilation functions
pub use compile::{
    compile_document_to_string, compile_html_to_document, compile_html_to_document_with_polyfill,
    compile_pdf_to_document, document_to_pdf_bytes,
};

// World (Typst compilation context)
pub use world::RheoWorld;

// Re-export reticulate module for spine building
pub use reticulate::spine::generate_bundle_entry;

// PDF utilities
pub use pdf_utils::DocumentTitle;

// Typst types (commonly used by plugins)
pub use typst_types::{
    EcoString, HeadingElem, HtmlDocument, NativeElement, OutlineNode, StyleChain, eco_format,
    eco_vec,
};

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

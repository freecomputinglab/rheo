pub mod assets;
pub mod build;
pub mod compile;
pub mod config;
pub mod constants;
pub mod diagnostics;
pub mod error;
pub mod html_utils;
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

// Note: Cli is now in rheo crate, not exported here

// === Core types (already exported) ===
pub use config::RheoConfig;
pub use constants::*;
pub use error::RheoError;
pub use globset::{Glob, GlobSet, GlobSetBuilder};
pub use manifest_version::ManifestVersion;
pub use path_utils::PathExt;
pub use results::{CompilationResults, FormatResult};

// === Plugin API re-exports ===

// Asset resolution
pub use assets::AssetResolver;
pub use build::{Build, BuildOptions};

// Compile options and context
pub use compile::{CompileUnit, RheoCompileOptions};

// Configuration types
pub use config::{AssetsField, PluginAssets, PluginSection, Spine};

// Plugin trait and context
pub use plugins::{
    AssetConfig, CompiledHtmlVertebra, FormatInitTemplate, FormatPlugin, LinkStrategy, OpenHandle,
    PackageAssets, PluginContext, ResolvedPackage, ServerHandle, SpineOptions,
};
pub use reticulate::types::RheoValue;

// HTML/PDF export utilities
pub use compile::{compile_document_to_string, document_to_pdf_bytes};

// World (Typst compilation context)
pub use world::RheoWorld;

// Re-export reticulate module for spine building
pub use reticulate::spine::BuiltSpine;

// PDF utilities
pub use pdf_utils::DocumentTitle;

// Typst types (commonly used by plugins)
pub use typst_types::{
    EcoString, HeadingElem, HtmlDocument, NativeElement, OutlineNode, StyleChain, eco_format,
    eco_vec,
};

/// Result type alias using RheoError
pub type Result<T> = std::result::Result<T, RheoError>;

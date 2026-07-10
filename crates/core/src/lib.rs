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
pub mod parser;
pub mod path_utils;
pub mod pdf_utils;
pub mod plugins;
pub mod results;
pub mod reticulate;
pub mod rheo_packages;
pub mod typst_types;
pub mod watch;
pub mod world;

// Note: Cli is now in rheo crate, not exported here

// === Core types (already exported) ===
pub use config::ManifestVersion;
pub use config::RheoConfig;
pub use constants::*;
pub use error::RheoError;
pub use globset::{Glob, GlobSet, GlobSetBuilder};
pub use path_utils::PathExt;
pub use results::{CompilationResults, FormatResult};

// === Plugin API re-exports ===

// Asset resolution
pub use assets::AssetResolver;
pub use build::{Build, BuildOptions};

// Configuration types
pub use config::{AssetsField, PluginAssets, PluginSection, Spine};

// Plugin trait and context
pub use parser::RheoValue;
pub use plugins::{
    AssetConfig, CastVertebra, FormatInitTemplate, FormatPlugin, OpenHandle, PackageAssets,
    PluginContext, ResolvedPackage, ServerHandle, SpineLayoutKind, SpineOptions, TypstFormat,
};

// HTML/PDF export utilities
pub use compile::{compile_document_to_string, document_to_pdf_bytes};
pub use html_utils::HtmlDom;

// World (Typst compilation context)
pub use world::RheoWorld;

// PDF utilities
pub use pdf_utils::DocumentTitle;

// Typst types (commonly used by plugins)
pub use typst_types::{
    Content, Document, EcoString, HeadingElem, HtmlDocument, Introspector, NativeElement,
    OutlineNode, StyleChain, eco_format, eco_vec,
};

/// Result type alias using RheoError
pub type Result<T> = std::result::Result<T, RheoError>;

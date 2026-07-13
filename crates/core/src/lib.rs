pub mod assets;
pub mod build;
pub mod compile;
pub mod config;
pub mod diagnostics;
pub mod packages;
pub mod parser;
pub mod plugins;
pub mod reticulate;
pub mod templates;
pub mod util;
pub mod world;

// Note: Cli is now in rheo crate, not exported here

// === Core types (already exported) ===
pub use config::ManifestVersion;
pub use config::RheoConfig;
pub use diagnostics::error::RheoError;
pub use diagnostics::results::{CompilationResults, FormatResult};
pub use globset::{Glob, GlobSet, GlobSetBuilder};
pub use util::constants::*;
pub use util::path::PathExt;

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
    PluginContext, ResolvedPackage, ServerHandle, SpineLayoutKind, TypstFormat,
};

// HTML/PDF export utilities
pub use compile::{compile_document_to_string, document_to_pdf_bytes};
pub use util::html::HtmlDom;

// World (Typst compilation context)
pub use world::RheoWorld;

// PDF utilities
pub use util::pdf::DocumentTitle;

// Typst types (commonly used by plugins)
pub use util::typst_types::{
    Content, Document, EcoString, HeadingElem, HtmlDocument, Introspector, NativeElement,
    OutlineNode, StyleChain, eco_format, eco_vec,
};

/// Result type alias using RheoError
pub type Result<T> = std::result::Result<T, RheoError>;

pub mod assets;
pub mod build;
pub mod compile;
pub mod config;
pub mod diagnostics;
pub mod html_dom;
pub mod output;
pub mod packages;
pub mod parser;
pub mod plugins;
pub mod project;
pub mod reticulate;
pub(crate) mod synth;
pub mod templates;
pub mod transclude;
pub mod util;
pub mod world;

// === Core types (already exported) ===
pub use config::ManifestVersion;
pub use config::RheoConfig;
pub use diagnostics::error::RheoError;
pub use diagnostics::results::{CompilationResults, FormatResult};
pub use globset::{Glob, GlobSet, GlobSetBuilder};
pub use util::constants::*;

// === Plugin API re-exports ===

// Asset resolution
pub use assets::AssetResolver;
pub use build::{Build, BuildOptions};

// Configuration types
pub use config::{AssetsField, PluginAssets, PluginSection, Spine};

// Plugin trait and context
pub use plugins::{
    Asset, AssetConfig, BundleInputs, CastVertebra, EmbeddedDefault, FormatInitTemplate,
    FormatPlugin, LiveReload, OpenHandle, PackageAssets, PageAssets, PluginContext,
    ResolvedPackage, ServedPage, ServerHandle, SpineLayoutKind, TypstFormat,
};
pub use transclude::ControlAssets;

// HTML/PDF export utilities
pub use compile::{compile_document_to_string, document_to_pdf_bytes};
pub use html_dom::HtmlDom;

// World (Typst compilation context)
pub use world::RheoWorld;

// Document title utilities
pub use reticulate::DocumentTitle;

// Typst types (commonly used by plugins)
pub use util::typst_types::{
    Content, Document, EcoString, HeadingElem, HtmlDocument, Introspector, NativeElement,
    OutlineNode, StyleChain, eco_format, eco_vec,
};

/// Result type alias using RheoError
pub type Result<T> = std::result::Result<T, RheoError>;

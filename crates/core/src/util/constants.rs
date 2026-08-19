/// File extension constants and shared regex patterns used throughout rheo
use regex::Regex;
use std::sync::LazyLock;

// File extensions
pub const TYP_EXT: &str = ".typ";

/// Filename, directly under `content_dir`, whose Typst is emitted as marrow — at
/// the bundle root, outside every document — rather than compiled as a vertebra.
pub const MARROW_FILE: &str = ".marrow.typ";

/// Prefix reserved for bundle assets consumed internally by rheo itself.
///
/// An `asset()` whose path starts with this prefix (e.g. `.rheo/head.html`) is
/// never written to a plugin's output directory, never embedded in a
/// container format (EPUB), and never served by the dev server — it is
/// stripped out and consumed by core before plugins see it. See
/// [`crate::transclude::ControlAssets`].
pub const CONTROL_ASSET_PREFIX: &str = ".rheo/";

/// Prefix reserved for the per-vertebra metadata beacon label rendered by
/// [`crate::util::typst_source::TypstStmt::MetadataBeacon`] (`<rheo-meta:<handle>>`).
/// An author-authored label starting with this prefix is a hard build error —
/// see [`crate::reticulate::spine::VirtualSpine::build`].
pub const RESERVED_META_LABEL_PREFIX: &str = "rheo-meta:";
pub const PDF_EXT: &str = ".pdf";
pub const HTML_EXT: &str = ".html";
pub const XHTML_EXT: &str = ".xhtml";
pub const EPUB_EXT: &str = ".epub";

/// Pattern for Typst label references: #label[text]
pub static TYPST_LABEL_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"#\w+\[([^\]]+)\]").expect("invalid TYPST_LABEL_PATTERN"));

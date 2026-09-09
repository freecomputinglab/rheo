/// File extension constants and shared regex patterns used throughout rheo
// File extensions
pub const TYP_EXT: &str = ".typ";

/// Filename, directly under `content_dir`, whose Typst is emitted as marrow — at
/// the bundle root, outside every document — rather than compiled as a vertebra.
///
/// Position-agnostic: a project's copy lands wherever `dot_marrow_is_epilogue`
/// says, a package's always in the epilogue. Either way the two explicit names
/// below outrank it.
pub const MARROW_FILE: &str = ".marrow.typ";

/// Marrow spliced before every document. Outranks [`MARROW_FILE`].
pub const MARROW_PRELUDE_FILE: &str = ".marrow.prelude.typ";

/// Marrow spliced after every document. Outranks [`MARROW_FILE`].
pub const MARROW_EPILOGUE_FILE: &str = ".marrow.epilogue.typ";

/// Every reserved marrow filename, for the scan that must keep all of them out
/// of the vertebra list.
pub const MARROW_RESERVED_FILES: [&str; 2] = [MARROW_PRELUDE_FILE, MARROW_EPILOGUE_FILE];

/// Prefix reserved for bundle assets consumed internally by rheo itself.
///
/// An `asset()` whose path starts with this prefix (e.g. `.rheo/head.html`) is
/// never written to a plugin's output directory, never embedded in a
/// container format (EPUB), and never served by the dev server — it is
/// stripped out and consumed by core before plugins see it. See
/// [`crate::transclude::ControlAssets`].
pub const CONTROL_ASSET_PREFIX: &str = ".rheo/";

/// Prefix reserved for the per-vertebra metadata beacon label rendered by
/// [`crate::synth::typst_source::TypstStmt::MetadataBeacon`] (`<rheo-meta:<handle>>`).
/// An author-authored label starting with this prefix is a hard build error —
/// see [`crate::reticulate::spine::VirtualSpine::build`].
pub const RESERVED_META_LABEL_PREFIX: &str = "rheo-meta:";

/// Project-root-relative path `RheoWorld` serves `typ/metadata.typ` under, for
/// the `#import "/<METADATA_MODULE_PATH>": ...` statements
/// [`crate::synth::typst_source::TypstStmt`]'s metadata-helper variants render.
pub const METADATA_MODULE_PATH: &str = "typ/metadata.typ";
pub const PDF_EXT: &str = ".pdf";
pub const HTML_EXT: &str = ".html";
pub const XHTML_EXT: &str = ".xhtml";
pub const EPUB_EXT: &str = ".epub";

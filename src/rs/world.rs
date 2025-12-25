use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{OutputFormat, Result, RheoError};
use chrono::{Datelike, Local};
use codespan_reporting::files::{Error as CodespanError, Files};
use parking_lot::Mutex;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Lines, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::download::Downloader;
use typst_kit::fonts::{FontSlot, Fonts};
use typst_kit::package::PackageStorage;
use typst_library::{Feature, Features};

/// A simple World implementation for rheo compilation.
pub struct RheoWorld {
    /// The root directory for resolving imports (document directory).
    root: PathBuf,

    /// The main file to compile.
    main: FileId,

    /// Typst's standard library.
    library: LazyHash<Library>,

    /// Metadata about discovered fonts.
    book: LazyHash<FontBook>,

    /// Locations of and storage for lazily loaded fonts.
    fonts: Vec<FontSlot>,

    /// Maps file ids to source files.
    slots: Mutex<HashMap<FileId, FileSlot>>,

    /// Package storage for downloading and caching packages.
    package_storage: PackageStorage,

    /// Output format for link transformations (None = no transformation).
    output_format: Option<OutputFormat>,
}

/// Holds the processed data for a file ID.
struct FileSlot {
    /// The loaded source file (for .typ files).
    source: Option<Source>,
    /// The loaded binary data (for other files).
    file: Option<Bytes>,
}

impl RheoWorld {
    /// Create a new world for compiling the given file.
    ///
    /// # Arguments
    /// * `root` - The root directory for resolving imports (document directory)
    /// * `main_file` - The main .typ file to compile
    /// * `output_format` - Output format for link transformations (None = no transformation)
    pub fn new(root: &Path, main_file: &Path, output_format: Option<OutputFormat>) -> Result<Self> {
        // Resolve paths
        let root = root.canonicalize().map_err(|e| {
            RheoError::path(
                root,
                format!("failed to canonicalize root directory: {}", e),
            )
        })?;
        let main_path = main_file.canonicalize().map_err(|e| {
            RheoError::path(
                main_file,
                format!("failed to canonicalize main file: {}", e),
            )
        })?;

        // Create virtual path for main file
        let main_vpath = VirtualPath::within_root(&main_path, &root).ok_or_else(|| {
            RheoError::path(&main_path, "main file must be within root directory")
        })?;
        let main = FileId::new(None, main_vpath);

        // Build library with HTML feature enabled
        let features: Features = [Feature::Html].into_iter().collect();
        let library = Library::builder().with_features(features).build();

        // Search for fonts using typst-kit
        // Respect TYPST_IGNORE_SYSTEM_FONTS for test consistency
        let include_system_fonts = std::env::var("TYPST_IGNORE_SYSTEM_FONTS").is_err();
        let font_search = Fonts::searcher()
            .include_system_fonts(include_system_fonts)
            .search();

        // Create package storage with default paths and downloader
        let package_storage = PackageStorage::new(
            None, // Use default cache directory
            None, // Use default data directory
            Downloader::new("rheo/0.1.0"),
        );

        Ok(Self {
            root,
            main,
            library: LazyHash::new(library),
            book: font_search.book.into(),
            fonts: font_search.fonts,
            slots: Mutex::new(HashMap::new()),
            package_storage,
            output_format,
        })
    }

    /// Reset the file cache for incremental compilation.
    ///
    /// This clears the cached source files and binary files, forcing them to be
    /// reloaded on the next access. Fonts, library, and package storage are preserved.
    ///
    /// This should be called before each recompilation in watch mode to ensure
    /// changed files are picked up while allowing Typst's comemo system to cache
    /// compilation results based on the actual file contents.
    pub fn reset(&self) {
        self.slots.lock().clear();
    }

    /// Change the main file for this world.
    ///
    /// This allows reusing the same World instance to compile different files
    /// in watch mode, which is more efficient than creating a new World for each file.
    ///
    /// # Arguments
    /// * `main_file` - The new main .typ file to compile
    pub fn set_main(&mut self, main_file: &Path) -> Result<()> {
        let main_path = main_file.canonicalize().map_err(|e| {
            RheoError::path(
                main_file,
                format!("failed to canonicalize main file: {}", e),
            )
        })?;

        let main_vpath = VirtualPath::within_root(&main_path, &self.root).ok_or_else(|| {
            RheoError::path(&main_path, "main file must be within root directory")
        })?;

        self.main = FileId::new(None, main_vpath);
        Ok(())
    }

    /// Transform links in source text based on output format.
    ///
    /// Applies AST-based link transformations:
    /// - HTML: .typ → .html
    /// - EPUB: .typ → .xhtml
    /// - PDF: Removes .typ links (or converts to labels if spine is provided)
    ///
    /// # Arguments
    /// * `text` - Source text to transform
    /// * `id` - File ID (for error reporting and path context)
    /// * `format` - Output format to transform for
    ///
    /// # Returns
    /// * `FileResult<String>` - Transformed source text
    fn transform_links(&self, text: &str, id: FileId, format: &OutputFormat) -> FileResult<String> {
        use crate::links::{parser, serializer, transformer};

        let source_obj = typst::syntax::Source::detached(text);
        let links = parser::extract_links(&source_obj);
        let code_ranges = serializer::find_code_block_ranges(&source_obj);

        let transformations = transformer::compute_transformations(
            &links,
            *format,
            None, // No spine for per-file transformations
            id.vpath().as_rootless_path(),
        )
        .map_err(|e| FileError::Other(Some(e.to_string().into())))?;

        Ok(serializer::apply_transformations(
            text,
            &transformations,
            &code_ranges,
        ))
    }

    /// Get the absolute path for a file ID.
    fn path_for_id(&self, id: FileId) -> FileResult<PathBuf> {
        // Special handling for stdin (which we don't support)
        if id.vpath().as_rooted_path().starts_with("<") {
            return Err(FileError::NotFound(
                id.vpath().as_rooted_path().display().to_string().into(),
            ));
        }

        // Handle package imports
        let mut root = &self.root;

        let buf;
        if let Some(spec) = id.package() {
            // Download and prepare the package if needed
            buf = self
                .package_storage
                .prepare_package(spec, &mut PrintDownload::new(spec))?;
            root = &buf;
        }

        // Construct path relative to root (or package root)
        let path = id.vpath().resolve(root).ok_or_else(|| {
            FileError::NotFound(id.vpath().as_rooted_path().display().to_string().into())
        })?;

        // If the file doesn't exist at the resolved location, try the document directory
        // This handles cases where templates in subdirectories (or packages) reference
        // user files that are in the document root (like references.bib)
        if !path.exists() {
            // Try resolving relative to document root
            if let Some(doc_path) = id.vpath().resolve(&self.root)
                && doc_path.exists()
            {
                return Ok(doc_path);
            }

            // If still not found, try just the filename in the document root
            // This handles "./references.bib" in lib/template.typ referring to ../references.bib
            if let Some(filename) = id.vpath().as_rooted_path().file_name() {
                let filename_path = self.root.join(filename);
                if filename_path.exists() {
                    return Ok(filename_path);
                }
            }
        }

        Ok(path)
    }

    /// Look up the lines of a source file.
    ///
    /// This is used by the codespan-reporting integration to provide source
    /// context when displaying diagnostics. Returns the lines from either the
    /// cached source or loaded file bytes.
    pub fn lookup(&self, id: FileId) -> Lines<String> {
        // Try to get from source cache first
        if let Some(slot) = self.slots.lock().get(&id)
            && let Some(source) = &slot.source
        {
            return source.lines().clone();
        }

        // Try to load the source using World trait
        if let Ok(source) = World::source(self, id) {
            return source.lines().clone();
        }

        // If source loading failed, try to get as bytes and convert
        if let Some(slot) = self.slots.lock().get(&id)
            && let Some(bytes) = &slot.file
        {
            // Attempt to convert bytes to Lines
            if let Ok(lines) = Lines::try_from(bytes) {
                return lines;
            }
        }

        // Last resort: return empty Lines
        Lines::new(String::new())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl World for RheoWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        // Check cache first
        if let Some(slot) = self.slots.lock().get(&id)
            && let Some(source) = &slot.source
        {
            return Ok(source.clone());
        }

        // Load from file system
        let path = self.path_for_id(id)?;
        let mut text = fs::read_to_string(&path).map_err(|e| FileError::from_io(e, &path))?;

        // For the main file, inject the rheo.typ template automatically
        if id == self.main {
            // Embed rheo.typ content directly (it's small and avoids path issues)
            let rheo_content = include_str!("../typ/rheo.typ");
            let template_inject = format!("{}\n#show: rheo_template\n\n", rheo_content);
            text = format!("{}{}", template_inject, text);
        }

        // Apply link transformations for ALL .typ files if output format is set
        if let Some(format) = &self.output_format {
            text = self.transform_links(&text, id, format)?;
        }

        let source = Source::new(id, text);

        // Cache the source
        self.slots.lock().entry(id).or_insert_with(|| FileSlot {
            source: Some(source.clone()),
            file: None,
        });

        Ok(source)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        // Check cache first
        if let Some(slot) = self.slots.lock().get(&id)
            && let Some(file) = &slot.file
        {
            return Ok(file.clone());
        }

        // Load from file system
        let path = self.path_for_id(id)?;
        let data = fs::read(&path).map_err(|e| FileError::from_io(e, &path))?;

        let bytes = Bytes::new(data);

        // Cache the file
        self.slots.lock().entry(id).or_insert_with(|| FileSlot {
            source: None,
            file: Some(bytes.clone()),
        });

        Ok(bytes)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index)?.get()
    }

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        let now = Local::now();

        // The time with the specified UTC offset, or within the local time zone.
        let with_offset = match offset {
            None => now,
            Some(hours) => {
                let offset_duration = chrono::Duration::hours(hours);
                now + offset_duration
            }
        };

        Datetime::from_ymd(
            with_offset.year(),
            with_offset.month().try_into().ok()?,
            with_offset.day().try_into().ok()?,
        )
    }
}

/// Implement the Files trait from codespan-reporting for diagnostic rendering.
///
/// This allows RheoWorld to provide file information (name, source lines, line ranges)
/// to codespan-reporting's diagnostic formatter.
impl<'a> Files<'a> for RheoWorld {
    type FileId = FileId;
    type Name = String;
    type Source = Lines<String>;

    fn name(&'a self, id: FileId) -> std::result::Result<Self::Name, CodespanError> {
        let vpath = id.vpath();
        Ok(if let Some(package) = id.package() {
            // For package files, show package name + path
            format!("{package}{}", vpath.as_rooted_path().display())
        } else {
            // For local files, try to show relative path from root
            vpath
                .resolve(&self.root)
                .and_then(|abs| pathdiff::diff_paths(abs, &self.root))
                .as_deref()
                .unwrap_or_else(|| vpath.as_rootless_path())
                .to_string_lossy()
                .into()
        })
    }

    fn source(&'a self, id: FileId) -> std::result::Result<Self::Source, CodespanError> {
        Ok(self.lookup(id))
    }

    fn line_index(&'a self, id: FileId, given: usize) -> std::result::Result<usize, CodespanError> {
        let source = self.lookup(id);
        source
            .byte_to_line(given)
            .ok_or_else(|| CodespanError::IndexTooLarge {
                given,
                max: source.len_bytes(),
            })
    }

    fn line_range(
        &'a self,
        id: FileId,
        given: usize,
    ) -> std::result::Result<std::ops::Range<usize>, CodespanError> {
        let source = self.lookup(id);
        source
            .line_to_range(given)
            .ok_or_else(|| CodespanError::LineTooLarge {
                given,
                max: source.len_lines(),
            })
    }
}

/// Progress tracker that logs package downloads using tracing.
struct PrintDownload {
    package_name: String,
}

impl PrintDownload {
    fn new(spec: &typst::syntax::package::PackageSpec) -> Self {
        Self {
            package_name: format!("{}@{}", spec.name, spec.version),
        }
    }
}

impl typst_kit::download::Progress for PrintDownload {
    fn print_start(&mut self) {
        tracing::info!("downloading package {}", self.package_name);
    }

    fn print_progress(&mut self, state: &typst_kit::download::DownloadState) {
        if let Some(total) = state.content_len {
            let percent = (state.total_downloaded as f64 / total as f64 * 100.0) as u32;
            tracing::debug!(
                "downloading package {} - {}% ({}/{})",
                self.package_name,
                percent,
                state.total_downloaded,
                total
            );
        } else {
            tracing::debug!(
                "downloading package {} - {} bytes",
                self.package_name,
                state.total_downloaded
            );
        }
    }

    fn print_finish(&mut self, state: &typst_kit::download::DownloadState) {
        tracing::info!(
            "downloaded package {} ({} bytes)",
            self.package_name,
            state.total_downloaded
        );
    }
}

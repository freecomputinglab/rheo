use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Result, RheoError};
use chrono::{Datelike, Local};
use parking_lot::Mutex;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualPath};
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

    /// The repository root directory (for future use).
    #[allow(dead_code)]
    repo_root: PathBuf,

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

    /// Whether to remove relative .typ links from the main file (for PDF/EPUB).
    remove_typ_links: bool,
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
    /// * `repo_root` - The repository root directory (for rheo.typ imports)
    /// * `remove_typ_links` - Whether to remove relative .typ links (for PDF/EPUB)
    pub fn new(
        root: &Path,
        main_file: &Path,
        repo_root: &Path,
        remove_typ_links: bool,
    ) -> Result<Self> {
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
        let repo_root = repo_root.canonicalize().map_err(|e| {
            RheoError::path(
                repo_root,
                format!("failed to canonicalize repo root: {}", e),
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
        let font_search = Fonts::searcher()
            .include_system_fonts(true)
            .include_embedded_fonts(true)
            .search();

        // Create package storage with default paths and downloader
        let package_storage = PackageStorage::new(
            None, // Use default cache directory
            None, // Use default data directory
            Downloader::new("rheo/0.1.0"),
        );

        Ok(Self {
            root,
            repo_root,
            main,
            library: LazyHash::new(library),
            book: font_search.book,
            fonts: font_search.slots,
            slots: Mutex::new(HashMap::new()),
            package_storage,
            remove_typ_links,
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

    /// Calculate relative path from root to repo_root for rheo.typ import.
    #[allow(dead_code)]
    fn rheo_import_path(&self) -> Result<String> {
        let rheo_typ = self.repo_root.join("src/typ/rheo.typ");

        // Calculate relative path from root to rheo.typ
        let rel_path = pathdiff::diff_paths(&rheo_typ, &self.root).ok_or_else(|| {
            RheoError::path(&rheo_typ, "failed to calculate relative path to rheo.typ")
        })?;

        // Convert to Typst import format (forward slashes, must start with ./)
        let mut path_str = rel_path
            .to_str()
            .ok_or_else(|| RheoError::path(&rel_path, "path contains invalid UTF-8"))?
            .replace('\\', "/");

        // Ensure path starts with ./ for relative imports
        if !path_str.starts_with("./") && !path_str.starts_with("../") {
            path_str = format!("./{}", path_str);
        }

        Ok(path_str)
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
            if let Some(doc_path) = id.vpath().resolve(&self.root) {
                if doc_path.exists() {
                    return Ok(doc_path);
                }
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
        if let Some(slot) = self.slots.lock().get(&id) {
            if let Some(source) = &slot.source {
                return Ok(source.clone());
            }
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

            // Remove relative .typ links if requested (for PDF/EPUB)
            if self.remove_typ_links {
                text = crate::compile::remove_relative_typ_links(&text);
            }
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
        if let Some(slot) = self.slots.lock().get(&id) {
            if let Some(file) = &slot.file {
                return Ok(file.clone());
            }
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

/// Silent progress tracker for package downloads (kept for future use).
#[allow(dead_code)]
struct SilentProgress;

impl typst_kit::download::Progress for SilentProgress {
    fn print_start(&mut self) {
        // Silent - no output
    }

    fn print_progress(&mut self, _state: &typst_kit::download::DownloadState) {
        // Silent - no output
    }

    fn print_finish(&mut self, _state: &typst_kit::download::DownloadState) {
        // Silent - no output
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

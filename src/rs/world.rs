use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

use crate::{Result, RheoError};
use chrono::{Datelike, Local};
use parking_lot::Mutex;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::fonts::{FontSlot, Fonts};
use typst_kit::package::PackageStorage;
use typst_kit::download::Downloader;
use typst_library::{Feature, Features};

/// A simple World implementation for rheo compilation.
pub struct RheoWorld {
    /// The root directory for resolving imports.
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
    pub fn new(root: &Path, main_file: &Path) -> Result<Self> {
        // Resolve paths
        let root = root.canonicalize()
            .map_err(|e| RheoError::path(root, format!("failed to canonicalize root directory: {}", e)))?;
        let main_path = main_file.canonicalize()
            .map_err(|e| RheoError::path(main_file, format!("failed to canonicalize main file: {}", e)))?;

        // Create virtual path for main file
        let main_vpath = VirtualPath::within_root(&main_path, &root)
            .ok_or_else(|| RheoError::path(&main_path, "main file must be within root directory"))?;
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
            None,  // Use default cache directory
            None,  // Use default data directory
            Downloader::new("rheo/0.1.0"),
        );

        Ok(Self {
            root,
            main,
            library: LazyHash::new(library),
            book: LazyHash::new(font_search.book),
            fonts: font_search.fonts,
            slots: Mutex::new(HashMap::new()),
            package_storage,
        })
    }

    /// Get the absolute path for a file ID.
    fn path_for_id(&self, id: FileId) -> FileResult<PathBuf> {
        // Special handling for stdin (which we don't support)
        if id.vpath().as_rooted_path().starts_with("<") {
            return Err(FileError::NotFound(id.vpath().as_rooted_path().display().to_string().into()));
        }

        // Handle package imports
        let mut root = &self.root;
        let mut buf = PathBuf::new();

        if let Some(spec) = id.package() {
            // Download and prepare the package if needed
            buf = self.package_storage
                .prepare_package(spec, &mut PrintDownload::new(spec))?;
            root = &buf;
        }

        // Construct path relative to root (or package root)
        let path = id.vpath().resolve(root)
            .ok_or_else(|| FileError::NotFound(id.vpath().as_rooted_path().display().to_string().into()))?;

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
        let mut text = fs::read_to_string(&path)
            .map_err(|e| FileError::from_io(e, &path))?;

        // For the main file, inject the rheo.typ import and template automatically
        if id == self.main {
            let import_statement = "#import \"/src/typst/rheo.typ\": *\n#show: rheo_template\n\n";
            text = format!("{}{}", import_statement, text);
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
        let data = fs::read(&path)
            .map_err(|e| FileError::from_io(e, &path))?;

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

/// Silent progress tracker for package downloads.
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

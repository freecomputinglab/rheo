use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Result, RheoError};
use chrono::{Datelike, Local};
use codespan_reporting::files::{Error as CodespanError, Files};
use parking_lot::Mutex;
use tracing::warn;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Dict, IntoValue};
use typst::syntax::{FileId, Lines, Source, VirtualPath};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::download::Downloader;
use typst_kit::fonts::{FontSlot, Fonts};
use typst_kit::package::PackageStorage;
use typst_library::{Feature, Features};

/// Build sys.inputs Dict for Typst compilation.
fn build_inputs(format_name: Option<&str>) -> Dict {
    let mut dict = Dict::new();
    if let Some(name) = format_name {
        dict.insert("rheo-target".into(), name.into_value());
    }
    dict
}

/// A simple World implementation for rheo compilation.
pub struct RheoWorld {
    root: PathBuf,
    main: FileId,
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<FontSlot>,
    slots: Mutex<HashMap<FileId, FileSlot>>,
    package_storage: PackageStorage,
    /// Output format name for link transformations and polyfill injection.
    /// None = no transformation.
    format_name: Option<String>,
}

struct FileSlot {
    source: Option<Source>,
    file: Option<Bytes>,
}

impl RheoWorld {
    /// Create a new world for compiling the given file.
    ///
    /// # Arguments
    /// * `root` - The root directory for resolving imports
    /// * `main_file` - The main .typ file to compile
    /// * `format_name` - Plugin name for link transformations (e.g. "pdf", "html", "epub"; None = no transformation)
    pub fn new(root: &Path, main_file: &Path, format_name: Option<&str>) -> Result<Self> {
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

        let main_vpath = VirtualPath::within_root(&main_path, &root).ok_or_else(|| {
            RheoError::path(&main_path, "main file must be within root directory")
        })?;
        let main = FileId::new(None, main_vpath);

        let features: Features = [Feature::Html].into_iter().collect();
        let inputs = build_inputs(format_name);
        let library = Library::builder()
            .with_features(features)
            .with_inputs(inputs)
            .build();

        let include_system_fonts = std::env::var("TYPST_IGNORE_SYSTEM_FONTS").is_err();
        let font_search = Fonts::searcher()
            .include_system_fonts(include_system_fonts)
            .search();

        let package_storage = PackageStorage::new(
            None,
            None,
            Downloader::new(concat!("rheo/", env!("CARGO_PKG_VERSION"))),
        );

        Ok(Self {
            root,
            main,
            library: LazyHash::new(library),
            book: font_search.book.into(),
            fonts: font_search.fonts,
            slots: Mutex::new(HashMap::new()),
            package_storage,
            format_name: format_name.map(str::to_string),
        })
    }

    /// Reset the file cache for incremental compilation.
    pub fn reset(&self) {
        self.slots.lock().clear();
    }

    /// Change the main file for this world.
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

    /// Transform links in source text based on output format name.
    fn transform_links(&self, text: &str, id: FileId, format_name: &str) -> FileResult<String> {
        use crate::reticulate::transformer::LinkTransformer;

        let transformer = LinkTransformer::new(format_name);
        transformer
            .transform_source(text, id.vpath().as_rootless_path(), &self.root)
            .map_err(|e| FileError::Other(Some(e.to_string().into())))
    }

    fn path_for_id(&self, id: FileId) -> FileResult<PathBuf> {
        if id.vpath().as_rooted_path().starts_with("<") {
            return Err(FileError::NotFound(
                id.vpath().as_rooted_path().display().to_string().into(),
            ));
        }

        let mut root = &self.root;

        let buf;
        if let Some(spec) = id.package() {
            buf = self
                .package_storage
                .prepare_package(spec, &mut PrintDownload::new(spec))?;
            root = &buf;
        }

        let path = id.vpath().resolve(root).ok_or_else(|| {
            FileError::NotFound(id.vpath().as_rooted_path().display().to_string().into())
        })?;

        if !path.exists() {
            // Fallback 1: Resolve against project root instead of package root.
            // Handles the case where a file path in the project is incorrectly
            // specified as a package path, or package resolution fails.
            if let Some(doc_path) = id.vpath().resolve(&self.root)
                && doc_path.exists()
            {
                return Ok(doc_path);
            }

            // Fallback 2 (last resort): Look for just the filename at project root.
            // This strips all directory components and can silently load the wrong
            // file if the intended file doesn't exist. For example, if importing
            // `chapters/intro.typ` fails but `intro.typ` exists at root, this will
            // load the wrong file.
            if let Some(filename) = id.vpath().as_rooted_path().file_name() {
                let filename_path = self.root.join(filename);
                if filename_path.exists() {
                    // Log a warning so this fallback is visible in verbose mode
                    warn!(
                        requested = %id.vpath().as_rooted_path().display(),
                        loaded = %filename_path.display(),
                        "path resolution fallback: using filename from project root"
                    );
                    return Ok(filename_path);
                }
            }
        }

        Ok(path)
    }

    pub fn lookup(&self, id: FileId) -> Lines<String> {
        if let Some(slot) = self.slots.lock().get(&id)
            && let Some(source) = &slot.source
        {
            return source.lines().clone();
        }

        if let Ok(source) = World::source(self, id) {
            return source.lines().clone();
        }

        if let Some(slot) = self.slots.lock().get(&id)
            && let Some(bytes) = &slot.file
            && let Ok(lines) = Lines::try_from(bytes)
        {
            return lines;
        }

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
        if let Some(slot) = self.slots.lock().get(&id)
            && let Some(source) = &slot.source
        {
            return Ok(source.clone());
        }

        let path = self.path_for_id(id)?;
        let mut text = fs::read_to_string(&path).map_err(|e| FileError::from_io(e, &path))?;

        // Inject target() polyfill into ALL .typ files for EPUB compilation.
        let target_polyfill = if self.format_name.as_deref() == Some("epub") {
            "// Polyfill target() to return rheo's output format from sys.inputs\n\
             #let target() = if \"rheo-target\" in sys.inputs { sys.inputs.rheo-target } else { std.target() }\n\n"
        } else {
            ""
        };

        // For the main file, also inject the rheo.typ template.
        if id == self.main {
            let rheo_content = include_str!("typ/rheo.typ");
            let template_inject = format!(
                "{}{}\n#show: rheo_template\n\n",
                target_polyfill, rheo_content
            );
            text = format!("{}{}", template_inject, text);
        } else if !target_polyfill.is_empty() {
            text = format!("{}{}", target_polyfill, text);
        }

        // Apply link transformations for ALL .typ files if output format is set.
        if let Some(ref name) = self.format_name {
            text = self.transform_links(&text, id, name)?;
        }

        let source = Source::new(id, text);
        self.slots.lock().entry(id).or_insert_with(|| FileSlot {
            source: Some(source.clone()),
            file: None,
        });

        Ok(source)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if let Some(slot) = self.slots.lock().get(&id)
            && let Some(file) = &slot.file
        {
            return Ok(file.clone());
        }

        let path = self.path_for_id(id)?;
        let data = fs::read(&path).map_err(|e| FileError::from_io(e, &path))?;
        let bytes = Bytes::new(data);

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

impl<'a> Files<'a> for RheoWorld {
    type FileId = FileId;
    type Name = String;
    type Source = Lines<String>;

    fn name(&'a self, id: FileId) -> std::result::Result<Self::Name, CodespanError> {
        let vpath = id.vpath();
        Ok(if let Some(package) = id.package() {
            format!("{package}{}", vpath.as_rooted_path().display())
        } else {
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

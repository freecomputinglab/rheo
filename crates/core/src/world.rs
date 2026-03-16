use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Result, RheoError};
use chrono::{Datelike, Local};
use codespan_reporting::files::{Error as CodespanError, Files};
use parking_lot::Mutex;
use tracing::warn;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Lines, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::downloader::SystemDownloader;
use typst_kit::fonts::FontStore;
use typst_kit::packages::SystemPackages;
use typst_library::{Feature, Features};

/// A simple World implementation for rheo compilation.
pub struct RheoWorld {
    root: PathBuf,
    main: FileId,
    library: LazyHash<Library>,
    font_store: FontStore,
    slots: Mutex<HashMap<FileId, FileSlot>>,
    packages: SystemPackages,
    /// Plugin-contributed Typst library code, injected after core prelude.
    plugin_library: Option<String>,
    /// When true, inject `#let target() = "epub"` polyfill into all .typ files.
    /// Set by the EPUB plugin before compilation.
    pub epub_polyfill_mode: bool,
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
    /// * `plugin_library` - Optional plugin-contributed Typst library code to inject after core prelude
    pub fn new(root: &Path, main_file: &Path, plugin_library: Option<String>) -> Result<Self> {
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

        let main_vpath = VirtualPath::virtualize(&root, &main_path)
            .map_err(|_| RheoError::path(&main_path, "main file must be within root directory"))?;
        let rooted_path = RootedPath::new(VirtualRoot::Project, main_vpath);
        let main = rooted_path.intern();

        // Feature::Bundle enables the bundle API for PDF and HTML compilation.
        // EPUB does not use bundle compilation; it compiles each spine file separately
        // and merges them into a single .epub, which is a different architectural approach.
        let features: Features = [Feature::Html, Feature::Bundle].into_iter().collect();
        let library = Library::builder().with_features(features).build();

        let mut font_store = FontStore::new();
        let include_system_fonts = std::env::var("TYPST_IGNORE_SYSTEM_FONTS").is_err();
        if include_system_fonts {
            font_store.extend(typst_kit::fonts::system());
        }
        font_store.extend(typst_kit::fonts::embedded());

        let downloader = SystemDownloader::new(concat!("rheo/", env!("CARGO_PKG_VERSION")));
        let packages = SystemPackages::new(downloader);

        Ok(Self {
            root,
            main,
            library: LazyHash::new(library),
            font_store,
            slots: Mutex::new(HashMap::new()),
            packages,
            plugin_library,
            epub_polyfill_mode: false,
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

        let main_vpath = VirtualPath::virtualize(&self.root, &main_path)
            .map_err(|_| RheoError::path(&main_path, "main file must be within root directory"))?;
        let rooted_path = RootedPath::new(VirtualRoot::Project, main_vpath);
        self.main = rooted_path.intern();
        Ok(())
    }

    /// Inject a generated bundle entry as a virtual main file.
    ///
    /// Pre-populates `self.slots` so `World::source()` returns the provided source
    /// immediately (bypassing template injection — preamble is already baked in).
    /// Sets `self.main` to the virtual FileId.
    pub fn inject_bundle_entry(&mut self, source: String) -> FileId {
        let vpath = VirtualPath::new("__rheo_bundle_entry__.typ")
            .expect("static bundle entry path is valid");
        let virtual_id = RootedPath::new(VirtualRoot::Project, vpath).intern();
        let typst_source = Source::new(virtual_id, source);
        self.slots.lock().insert(
            virtual_id,
            FileSlot {
                source: Some(typst_source),
                file: None,
            },
        );
        self.main = virtual_id;
        virtual_id
    }

    fn path_for_id(&self, id: FileId) -> FileResult<PathBuf> {
        if id.vpath().get_with_slash().starts_with('<') {
            return Err(FileError::NotFound(
                id.vpath().get_with_slash().to_string().into(),
            ));
        }

        let root = &self.root;

        let buf;
        let root = match id.root() {
            typst::syntax::VirtualRoot::Project => root,
            typst::syntax::VirtualRoot::Package(spec) => {
                buf = self
                    .packages
                    .obtain(spec)
                    .map_err(|e| FileError::Other(Some(e.to_string().into())))?
                    .path()
                    .to_path_buf();
                &buf
            }
        };

        let path = id.vpath().realize(root);

        if !path.exists() {
            // Fallback 1: Resolve against project root instead of package root.
            // Handles the case where a file path in the project is incorrectly
            // specified as a package path, or package resolution fails.
            let doc_path = id.vpath().realize(&self.root);
            if doc_path.exists() {
                return Ok(doc_path);
            }

            // Fallback 2 (last resort): Look for just the filename at project root.
            // This strips all directory components and can silently load the wrong
            // file if the intended file doesn't exist. For example, if importing
            // `chapters/intro.typ` fails but `intro.typ` exists at root, this will
            // load the wrong file.
            let vpath_str = id.vpath().get_without_slash();
            if let Some(filename) = PathBuf::from(vpath_str).file_name() {
                let filename_path = self.root.join(filename);
                if filename_path.exists() {
                    // Log a warning so this fallback is visible in verbose mode
                    warn!(
                        requested = %id.vpath().get_with_slash(),
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
        // First lock: check for cached source
        {
            let slots = self.slots.lock();
            if let Some(slot) = slots.get(&id)
                && let Some(source) = &slot.source
            {
                return source.lines().clone();
            }
        }

        // Drop lock before calling World::source (may acquire its own locks)
        if let Ok(source) = World::source(self, id) {
            return source.lines().clone();
        }

        // Second lock: check for cached file
        let slots = self.slots.lock();
        if let Some(slot) = slots.get(&id)
            && let Some(bytes) = &slot.file
        {
            // Convert bytes to string for line tracking
            if let Ok(text) = std::str::from_utf8(bytes.as_slice()) {
                return Lines::new(text.to_string());
            }
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
        self.font_store.book()
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

        // EPUB polyfill: inject target() polyfill into all .typ files when epub_polyfill_mode is set.
        // The EPUB plugin sets this flag before compilation to enable polyfill injection.
        let epub_polyfill = if self.epub_polyfill_mode {
            "#let target() = \"epub\"\n\n"
        } else {
            ""
        };

        // For the main file, inject the rheo.typ template and plugin library code.
        if id == self.main {
            let rheo_content = include_str!("typ/rheo.typ");
            let plugin_lib_content = self.plugin_library.as_deref().unwrap_or("");
            let template_inject = format!(
                "{}{}\n#show: rheo_template\n\n",
                rheo_content, plugin_lib_content
            );
            text = format!("{}{}", template_inject, text);
        } else if !epub_polyfill.is_empty() {
            text = format!("{}{}", epub_polyfill, text);
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
        self.font_store.font(index)
    }

    fn today(&self, offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        let now = Local::now();
        let with_offset = match offset {
            None => now,
            Some(duration) => {
                // Convert typst::foundations::Duration to time::Duration
                let time_duration: time::Duration = duration.into();
                // Convert time::Duration to chrono::Duration
                let chrono_duration = chrono::Duration::seconds(time_duration.whole_seconds())
                    + chrono::Duration::nanoseconds(time_duration.subsec_nanoseconds() as i64);
                now + chrono_duration
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
        Ok(match id.root() {
            typst::syntax::VirtualRoot::Package(spec) => {
                format!("{spec}{}", vpath.get_with_slash())
            }
            typst::syntax::VirtualRoot::Project => {
                let abs = vpath.realize(&self.root);
                match pathdiff::diff_paths(abs, &self.root) {
                    Some(diff) => diff.as_path().to_string_lossy().into(),
                    None => PathBuf::from(vpath.get_without_slash())
                        .as_path()
                        .to_string_lossy()
                        .into(),
                }
            }
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

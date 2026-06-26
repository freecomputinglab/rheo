use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::plugins::LinkStrategy;
use crate::rheo_packages::RheoPackages;
use crate::{Result, RheoError};
use chrono::{Datelike, Local};
use codespan_reporting::files::{Error as CodespanError, Files};
use parking_lot::Mutex;
use tracing::warn;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Dict, IntoValue};
use typst::syntax::{FileId, Lines, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::downloader::SystemDownloader;
use typst_kit::fonts::FontStore;
use typst_kit::packages::SystemPackages;
use typst_library::foundations::Duration;
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
    font_store: FontStore,
    package_storage: SystemPackages,
    rheo_packages: RheoPackages,
    slots: Mutex<HashMap<FileId, FileSlot>>,
    /// Output format name for link transformations and polyfill injection.
    /// None = no transformation.
    format_name: Option<String>,
    /// How relative `.typ` links are rewritten when `format_name` is set.
    link_strategy: LinkStrategy,
    /// Plugin-contributed Typst library code, injected after core prelude.
    plugin_library: Option<String>,
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
    /// * `link_strategy` - How relative `.typ` links are rewritten when `format_name` is set
    /// * `plugin_library` - Optional plugin-contributed Typst library code to inject after core prelude
    pub fn new(
        root: &Path,
        main_file: &Path,
        format_name: Option<&str>,
        link_strategy: LinkStrategy,
        plugin_library: Option<String>,
        font_dirs: Vec<PathBuf>,
    ) -> Result<Self> {
        let root = crate::path_utils::canonicalize_path(root)?;
        let main_path = crate::path_utils::canonicalize_path(main_file)?;

        let main_vpath = VirtualPath::virtualize(&root, &main_path).map_err(|e| {
            RheoError::path(
                &main_path,
                format!("main file must be within root directory: {}", e),
            )
        })?;
        let rooted_path = RootedPath::new(VirtualRoot::Project, main_vpath);
        let main = rooted_path.intern();

        let features: Features = [Feature::Html, Feature::Bundle].into_iter().collect();
        let inputs = build_inputs(format_name);
        let library = Library::builder()
            .with_features(features)
            .with_inputs(inputs)
            .build();

        let include_system_fonts = std::env::var("TYPST_IGNORE_SYSTEM_FONTS").is_err();
        if !font_dirs.is_empty() {
            tracing::info!(dirs = ?font_dirs, "loading fonts from {} additional directories", font_dirs.len());
        }

        let mut font_store = FontStore::new();
        font_store.extend(typst_kit::fonts::embedded());
        if include_system_fonts {
            font_store.extend(typst_kit::fonts::system());
        }
        for dir in &font_dirs {
            font_store.extend(typst_kit::fonts::scan(dir));
        }

        let user_agent = concat!("rheo/", env!("CARGO_PKG_VERSION"));
        let package_storage = SystemPackages::new(SystemDownloader::new(user_agent));
        let rheo_packages = RheoPackages::new(SystemDownloader::new(user_agent));

        Ok(Self {
            root,
            main,
            library: LazyHash::new(library),
            book: font_store.book().clone(),
            font_store,
            package_storage,
            rheo_packages,
            slots: Mutex::new(HashMap::new()),
            format_name: format_name.map(str::to_string),
            link_strategy,
            plugin_library,
        })
    }

    /// Reset the file cache for incremental compilation.
    pub fn reset(&self) {
        self.slots.lock().clear();
    }

    /// Change the main file for this world, invalidating only main-file-dependent slots.
    ///
    /// Only the outgoing and incoming main file slots are invalidated. All other slots
    /// (shared imports, packages) are deterministic given `format_name` and `root`, so
    /// they remain valid across per-file compilations.
    pub fn set_main(&mut self, main_file: &Path) -> Result<()> {
        let old_main = self.main;
        let main_path = crate::path_utils::canonicalize_path(main_file)?;

        let main_vpath = VirtualPath::virtualize(&self.root, &main_path).map_err(|e| {
            RheoError::path(
                &main_path,
                format!("main file must be within root directory: {}", e),
            )
        })?;
        let rooted_path = RootedPath::new(VirtualRoot::Project, main_vpath);
        self.main = rooted_path.intern();

        let mut slots = self.slots.lock();
        slots.remove(&old_main);
        slots.remove(&self.main);

        Ok(())
    }

    /// Transform links in source text based on output format name.
    fn transform_links(&self, text: &str, id: FileId, format_name: &str) -> FileResult<String> {
        use crate::reticulate::transformer::LinkTransformer;

        let transformer = LinkTransformer::new(format_name).with_strategy(self.link_strategy);
        transformer
            .transform_source(text, Path::new(id.vpath().get_without_slash()), &self.root)
            .map_err(|e| FileError::Other(Some(e.to_string().into())))
    }

    fn path_for_id(&self, id: FileId) -> FileResult<PathBuf> {
        if id.vpath().get_with_slash().starts_with("<") {
            return Err(FileError::NotFound(
                id.vpath().get_with_slash().to_string().into(),
            ));
        }

        if let VirtualRoot::Package(spec) = id.root() {
            let fs_root = if spec.namespace == "rheo" {
                self.rheo_packages.obtain(spec).map_err(FileError::from)?
            } else {
                self.package_storage.obtain(spec).map_err(FileError::from)?
            };
            return fs_root.resolve(id.vpath());
        }

        let path = id
            .vpath()
            .realize(&self.root)
            .map_err(|_| FileError::NotFound(id.vpath().get_with_slash().to_string().into()))?;

        if !path.exists() {
            // Fallback 1: Look for just the filename at project root.
            // This strips all directory components and can silently load the wrong
            // file if the intended file doesn't exist. For example, if importing
            // `chapters/intro.typ` fails but `intro.typ` exists at root, this will
            // load the wrong file.
            if let Some(filename) = Path::new(id.vpath().get_with_slash()).file_name() {
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
        {
            let text = std::str::from_utf8(bytes.as_slice()).unwrap_or("");
            return Lines::new(text.to_string());
        }

        Lines::new(String::new())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Compile the current main file to an HTML document.
    pub fn compile_html(&self) -> crate::Result<typst_html::HtmlDocument> {
        use crate::diagnostics::unwrap_compilation_result;
        use typst::diag::SourceDiagnostic;

        tracing::info!("compiling to HTML");
        let result = typst::compile::<typst_html::HtmlDocument>(self);
        let filter = |w: &SourceDiagnostic| {
            !w.message
                .contains("html export is under active development and incomplete")
        };
        unwrap_compilation_result(Some(self), result, Some(filter))
    }

    /// Compile the current main file to a paged (PDF) document.
    pub fn compile_pdf(&self) -> crate::Result<typst_layout::PagedDocument> {
        use crate::diagnostics::unwrap_compilation_result;

        tracing::info!("compiling to PDF");
        let result = typst::compile::<typst_layout::PagedDocument>(self);
        unwrap_compilation_result(Some(self), result, None::<fn(&_) -> bool>)
    }

    /// Compile the spine to its per-file outputs.
    ///
    /// Internally this drives Typst's multi-file bundle target, but that is an
    /// implementation detail: nothing user-facing references "bundle".
    pub fn compile_bundle(&self) -> crate::Result<typst_bundle::Bundle> {
        use crate::diagnostics::unwrap_compilation_result;
        use typst::diag::SourceDiagnostic;

        tracing::debug!("compiling spine via bundle target");
        let result = typst::compile::<typst_bundle::Bundle>(self);
        // Suppress Typst's experimental-feature notice; bundle is internal-only.
        let filter = |w: &SourceDiagnostic| !w.message.contains("bundle export is experimental");
        unwrap_compilation_result(Some(self), result, Some(filter))
    }

    /// Create a new world and compile the given file to an HTML document.
    pub fn compile_html_file(
        root: &Path,
        input: &Path,
        format_name: &str,
        link_strategy: LinkStrategy,
        plugin_library: Option<String>,
        font_dirs: Vec<PathBuf>,
    ) -> crate::Result<typst_html::HtmlDocument> {
        let world = Self::new(
            root,
            input,
            Some(format_name),
            link_strategy,
            plugin_library,
            font_dirs,
        )?;
        tracing::info!(input = %input.display(), "compiling to HTML");
        world.compile_html()
    }

    /// Create a new world and compile the given file to a paged (PDF) document.
    pub fn compile_pdf_file(
        root: &Path,
        input: &Path,
        format_name: Option<&str>,
        link_strategy: LinkStrategy,
        plugin_library: Option<String>,
        font_dirs: Vec<PathBuf>,
    ) -> crate::Result<typst_layout::PagedDocument> {
        let world = Self::new(
            root,
            input,
            format_name,
            link_strategy,
            plugin_library,
            font_dirs,
        )?;
        tracing::info!(input = %input.display(), "compiling to PDF");
        world.compile_pdf()
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

        // Inject target() polyfill for all plugin formats.
        let target_polyfill = if self.format_name.is_some() {
            "// Polyfill target() to return rheo's output format from sys.inputs\n\
             #let target() = if \"rheo-target\" in sys.inputs { sys.inputs.rheo-target } else { std.target() }\n\n"
        } else {
            ""
        };

        // For the main file, also inject the rheo.typ template and plugin library code.
        if id == self.main {
            let rheo_content = include_str!("typ/rheo.typ");
            let plugin_lib_content = self.plugin_library.as_deref().unwrap_or("");
            let template_inject = format!(
                "{}{}\n{}\n#show: rheo_template\n\n",
                target_polyfill, rheo_content, plugin_lib_content
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
        self.font_store.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        let now = Local::now();
        let with_offset = match offset {
            None => now,
            Some(duration) => now + chrono::Duration::seconds(duration.seconds() as i64),
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
        Ok(if let VirtualRoot::Package(package) = id.root() {
            format!("{package}{}", vpath.get_with_slash())
        } else {
            vpath
                .realize(&self.root)
                .ok()
                .and_then(|abs| pathdiff::diff_paths(abs, &self.root))
                .as_deref()
                .unwrap_or_else(|| Path::new(vpath.get_without_slash()))
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

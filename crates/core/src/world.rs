use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::packages::RheoPackages;
use crate::reticulate::VertebraInjection;
use crate::util::typst_literal::TypstLiteral;
use crate::util::typst_source::TypstStmt;
use crate::{Result, RheoError};
use chrono::{Datelike, Local};
use codespan_reporting::files::{Error as CodespanError, Files};
use parking_lot::Mutex;
use tracing::warn;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Dict};
use typst::syntax::{FileId, Lines, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::downloader::SystemDownloader;
use typst_kit::fonts::FontStore;
use typst_kit::packages::SystemPackages;
use typst_library::Feature;
use typst_library::foundations::Duration;

/// Build sys.inputs Dict for Typst compilation.
///
/// The output format is surfaced only via `rheo-context.target` (read by the
/// injected `target()` polyfill and the `rheo.typ` helpers); there is no
/// separate `rheo-target` key.
fn build_inputs(rheo_context: Option<&TypstLiteral>) -> Dict {
    let mut dict = Dict::new();
    if let Some(ctx) = rheo_context {
        dict.insert("rheo-context".into(), ctx.to_value());
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
    /// Output format name for polyfill injection.
    /// None = no polyfill.
    format_name: Option<String>,
    /// Plugin-contributed Typst library code, injected after core prelude.
    plugin_library: Option<String>,
    /// In-memory source for the main file. When set, the world serves this
    /// content for the main FileId instead of reading from disk.
    virtual_main_source: Option<String>,
    /// Per-vertebra source overlay from the Mould stage, keyed by project-relative
    /// include path (e.g. `content/intro.typ`). When an included file matches, the
    /// world serves the rewritten source instead of reading it from disk.
    source_overlay: HashMap<String, String>,
    /// Per-vertebra Typst injections (prelude + epilogue), keyed by
    /// project-relative include path. The prelude is prepended to each
    /// matching vertebra's served source so authored Typst can read
    /// `rheo-context` (its handle and the spine); the epilogue (a metadata
    /// beacon, when emitted for the layout) is appended after it.
    rheo_context: HashMap<String, VertebraInjection>,
}

struct FileSlot {
    source: Option<Source>,
    file: Option<Bytes>,
}

impl RheoWorld {
    /// Assemble a world once `root`, `main`, and the `sys.inputs` context are
    /// resolved — the part both constructors below share.
    #[allow(clippy::too_many_arguments)]
    fn assemble(
        root: PathBuf,
        main: FileId,
        inputs_context: Option<&TypstLiteral>,
        format_name: Option<&str>,
        plugin_library: Option<String>,
        virtual_main_source: Option<String>,
        source_overlay: HashMap<String, String>,
        rheo_context: HashMap<String, VertebraInjection>,
        font_dirs: Vec<PathBuf>,
    ) -> Self {
        let library = Library::builder()
            .with_features([Feature::Html, Feature::Bundle].into_iter().collect())
            .with_inputs(build_inputs(inputs_context))
            .build();

        let (font_store, package_storage, rheo_packages) = Self::init_resources(&font_dirs);

        Self {
            root,
            main,
            book: font_store.book().clone(),
            library: LazyHash::new(library),
            font_store,
            package_storage,
            rheo_packages,
            slots: Mutex::new(HashMap::new()),
            format_name: format_name.map(str::to_string),
            plugin_library,
            virtual_main_source,
            source_overlay,
            rheo_context,
        }
    }

    /// Create a new world for compiling a single file.
    pub fn new(
        root: &Path,
        main_file: &Path,
        format_name: Option<&str>,
        plugin_library: Option<String>,
        font_dirs: Vec<PathBuf>,
    ) -> Result<Self> {
        let root = crate::util::path::canonicalize_path(root)?;
        let main_path = crate::util::path::canonicalize_path(main_file)?;

        let main_vpath = VirtualPath::virtualize(&root, &main_path).map_err(|e| {
            RheoError::path(
                &main_path,
                format!("main file must be within root directory: {}", e),
            )
        })?;
        let main = RootedPath::new(VirtualRoot::Project, main_vpath).intern();

        Ok(Self::assemble(
            root,
            main,
            None,
            format_name,
            plugin_library,
            None,
            HashMap::new(),
            HashMap::new(),
            font_dirs,
        ))
    }

    /// Create a world for bundle spine compilation with a synthesized in-memory main.
    ///
    /// The caller passes the moulded spine: the synthesized main source, a
    /// per-vertebra source overlay (rewritten sources keyed by include path), and
    /// per-vertebra `rheo-context` injections (keyed the same way). The world
    /// serves the main for the synthetic main file ID, each overlay entry for
    /// its vertebra, and wraps the matching source with its prelude/epilogue —
    /// all bypassing disk. It also seeds `sys.inputs.rheo-context` with the
    /// global (spine) context, so packages can detect a rheo build and reach
    /// the shared spine without depending on the per-file `#let rheo-context`.
    pub fn new_for_bundle(
        root: &Path,
        virtual_main_source: String,
        source_overlay: HashMap<String, String>,
        rheo_context: HashMap<String, VertebraInjection>,
        global_context: Option<TypstLiteral>,
        format_name: Option<&str>,
        font_dirs: Vec<PathBuf>,
    ) -> Result<Self> {
        let root = crate::util::path::canonicalize_path(root)?;

        // Synthetic path — does not exist on disk; world serves it from memory.
        let main_vpath =
            VirtualPath::new("rheo_spine.typ").expect("rheo_spine.typ is a valid virtual path");
        let main = RootedPath::new(VirtualRoot::Project, main_vpath).intern();

        Ok(Self::assemble(
            root,
            main,
            global_context.as_ref(),
            format_name,
            None,
            Some(virtual_main_source),
            source_overlay,
            rheo_context,
            font_dirs,
        ))
    }

    fn init_resources(font_dirs: &[PathBuf]) -> (FontStore, SystemPackages, RheoPackages) {
        if !font_dirs.is_empty() {
            tracing::info!(dirs = ?font_dirs, "loading fonts from {} additional directories", font_dirs.len());
        }
        let include_system_fonts = std::env::var("TYPST_IGNORE_SYSTEM_FONTS").is_err();
        let mut font_store = FontStore::new();
        font_store.extend(typst_kit::fonts::embedded());
        if include_system_fonts {
            font_store.extend(typst_kit::fonts::system());
        }
        for dir in font_dirs {
            font_store.extend(typst_kit::fonts::scan(dir));
        }
        let user_agent = concat!("rheo/", env!("CARGO_PKG_VERSION"));
        let package_storage = SystemPackages::new(SystemDownloader::new(user_agent));
        let rheo_packages = RheoPackages::new(SystemDownloader::new(user_agent));
        (font_store, package_storage, rheo_packages)
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
        let main_path = crate::util::path::canonicalize_path(main_file)?;

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
        plugin_library: Option<String>,
        font_dirs: Vec<PathBuf>,
    ) -> crate::Result<typst_html::HtmlDocument> {
        let world = Self::new(root, input, Some(format_name), plugin_library, font_dirs)?;
        tracing::info!(input = %input.display(), "compiling to HTML");
        world.compile_html()
    }

    /// Create a new world and compile the given file to a paged (PDF) document.
    pub fn compile_pdf_file(
        root: &Path,
        input: &Path,
        format_name: Option<&str>,
        plugin_library: Option<String>,
        font_dirs: Vec<PathBuf>,
    ) -> crate::Result<typst_layout::PagedDocument> {
        let world = Self::new(root, input, format_name, plugin_library, font_dirs)?;
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

        // Serve the synthesized virtual main, then any moulded vertebra overlay,
        // from memory — falling back to disk for everything else.
        let mut text = if id == self.main
            && let Some(ref vm) = self.virtual_main_source
        {
            vm.clone()
        } else if let Some(body) = self
            .source_overlay
            .get(id.vpath().get_with_slash().trim_start_matches('/'))
        {
            body.clone()
        } else {
            let path = self.path_for_id(id)?;
            fs::read_to_string(&path).map_err(|e| FileError::from_io(e, &path))?
        };

        // Inject target() polyfill for all plugin formats.
        let target_polyfill = if self.format_name.is_some() {
            "// Polyfill target() to return rheo's output format from sys.inputs\n\
             #let target() = if \"rheo-context\" in sys.inputs and \"target\" in sys.inputs.rheo-context { sys.inputs.rheo-context.target } else { std.target() }\n\n"
        } else {
            ""
        };

        // For the main file, also inject the rheo.typ template and plugin library code.
        if id == self.main {
            let rheo_content = include_str!("typ/rheo.typ");
            let plugin_lib_content = self.plugin_library.as_deref().unwrap_or("");
            // `rheo-metadata`/`rheo-metadata-all` at bundle-root (marrow)
            // scope. `rheo-metadata` reuses `MetadataHelper`'s `Display` —
            // the same text the per-vertebra prelude injects — so the two
            // sites can't drift apart. `MetadataAllHelper` is marrow-only: a
            // single vertebra never needs "every vertebra's metadata at
            // once". No extra format gate is needed: `MetadataBeacon` (and
            // marrow itself) are only ever assembled for per-page (HTML/EPUB)
            // targets, since combined PDF hard-errors on
            // `document()`/`asset()` — the two already agree.
            let metadata_helper = TypstStmt::MetadataHelper;
            let metadata_all_helper = TypstStmt::MetadataAllHelper;
            let template_inject = format!(
                "{}{}\n\n{}\n\n{}\n\n{}\n#show: rheo_template\n\n",
                target_polyfill,
                rheo_content,
                metadata_helper,
                metadata_all_helper,
                plugin_lib_content
            );
            text = format!("{}{}", template_inject, text);
        } else {
            // Non-main files (vertebrae/partials): the target() polyfill, then any
            // per-vertebra `rheo-context` prelude, then the file's own source,
            // then any per-vertebra epilogue (the metadata beacon).
            let rel = id
                .vpath()
                .get_with_slash()
                .trim_start_matches('/')
                .to_string();
            let injection = self.rheo_context.get(&rel);
            let context_prelude = injection.map(|inj| inj.prelude.as_str()).unwrap_or("");
            let context_epilogue = injection.map(|inj| inj.epilogue.as_str()).unwrap_or("");
            if !target_polyfill.is_empty()
                || !context_prelude.is_empty()
                || !context_epilogue.is_empty()
            {
                text = format!(
                    "{}{}{}{}",
                    target_polyfill, context_prelude, text, context_epilogue
                );
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn serves_moulded_body_overlay_instead_of_disk() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Overlay carries a rewritten body; the file itself never exists on disk,
        // so serving it proves the overlay is consulted before the disk read.
        let mut source_overlay = HashMap::new();
        source_overlay.insert(
            "content/intro.typ".to_string(),
            "= Intro <intro:etal>\n".to_string(),
        );
        let world = RheoWorld::new_for_bundle(
            root,
            "#document(\"intro.html\", format: \"html\")[]".to_string(),
            source_overlay,
            HashMap::new(),
            None,
            Some("html"),
            vec![],
        )
        .unwrap();

        let id = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("content/intro.typ").unwrap(),
        )
        .intern();
        let source = World::source(&world, id).unwrap();
        assert!(source.text().contains("<intro:etal>"));
    }

    #[test]
    fn prepends_rheo_context_prelude_to_matching_vertebra() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // rheo-context is served from memory; the vertebra need not exist on disk.
        let mut source_overlay = HashMap::new();
        source_overlay.insert("content/intro.typ".to_string(), "= Intro\n".to_string());
        let mut rheo_context = HashMap::new();
        rheo_context.insert(
            "content/intro.typ".to_string(),
            VertebraInjection {
                prelude: "#let rheo-context() = (handle: \"intro\", ..sys.inputs.rheo-context)\n\n"
                    .to_string(),
                epilogue: String::new(),
            },
        );
        let world = RheoWorld::new_for_bundle(
            root,
            "#document(\"intro.html\", format: \"html\")[]".to_string(),
            source_overlay,
            rheo_context,
            None,
            Some("html"),
            vec![],
        )
        .unwrap();

        let id = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("content/intro.typ").unwrap(),
        )
        .intern();
        let text = World::source(&world, id).unwrap().text().to_string();
        // The prelude is prepended ahead of the file's own body.
        assert!(text.contains("#let rheo-context() = (handle: \"intro\""));
        assert!(text.find("rheo-context").unwrap() < text.find("= Intro").unwrap());
    }

    #[test]
    fn appends_rheo_context_epilogue_after_vertebra_body() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut source_overlay = HashMap::new();
        source_overlay.insert("content/intro.typ".to_string(), "= Intro\n".to_string());
        let mut rheo_context = HashMap::new();
        rheo_context.insert(
            "content/intro.typ".to_string(),
            VertebraInjection {
                prelude: "#let rheo-context() = (handle: \"intro\", ..sys.inputs.rheo-context)\n\n"
                    .to_string(),
                epilogue: "\n#metadata((handle: \"intro\")) <rheo-meta:intro>\n".to_string(),
            },
        );
        let world = RheoWorld::new_for_bundle(
            root,
            "#document(\"intro.html\", format: \"html\")[]".to_string(),
            source_overlay,
            rheo_context,
            None,
            Some("html"),
            vec![],
        )
        .unwrap();

        let id = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("content/intro.typ").unwrap(),
        )
        .intern();
        let text = World::source(&world, id).unwrap().text().to_string();
        // Order: prelude, then the vertebra's own body, then the epilogue.
        let prelude_at = text.find("#let rheo-context()").unwrap();
        let body_at = text.find("= Intro").unwrap();
        let epilogue_at = text.find("<rheo-meta:intro>").unwrap();
        assert!(prelude_at < body_at);
        assert!(body_at < epilogue_at);
    }

    #[test]
    fn build_inputs_includes_rheo_context_and_no_rheo_target() {
        let ctx = TypstLiteral::Dict(vec![
            ("spine".to_string(), TypstLiteral::Array(vec![])),
            ("target".to_string(), TypstLiteral::str("html")),
        ]);
        let dict = build_inputs(Some(&ctx));
        assert!(dict.contains("rheo-context"));
        // The deprecated top-level key is gone; format lives in rheo-context.target.
        assert!(!dict.contains("rheo-target"));

        let empty = build_inputs(None);
        assert!(!empty.contains("rheo-context"));
        assert!(!empty.contains("rheo-target"));
    }

    /// `#set document(date: auto)` stays `Smart::Auto` in the bundle's real
    /// `DocumentInfo`; `datetime.today()` resolves to a concrete, build-time
    /// date. Anything that syndicates a vertebra's date (e.g. a feed
    /// package) sees `datetime.today()` change on every build.
    #[test]
    fn document_date_auto_and_today_resolve_in_bundle_info() {
        use typst::foundations::Smart;
        use typst_bundle::BundleFile;
        use typst_library::model::Document;

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let main = r#"
#document("auto.html", format: "html")[
  #set document(date: auto)
  Auto date page.
]

#document("today.html", format: "html")[
  #set document(date: datetime.today())
  Today date page.
]
"#
        .to_string();

        let world = RheoWorld::new_for_bundle(
            root,
            main,
            HashMap::new(),
            HashMap::new(),
            None,
            Some("html"),
            vec![],
        )
        .unwrap();

        let bundle = world.compile_bundle().unwrap();

        let mut checked_auto = false;
        let mut checked_today = false;
        for (path, file) in bundle.files.iter() {
            let BundleFile::Document(doc) = file else {
                continue;
            };
            let info = doc.info();
            if path.get_without_slash().contains("auto") {
                assert_eq!(
                    info.date,
                    Smart::Auto,
                    "`date: auto` should stay Smart::Auto in DocumentInfo, not resolve to a concrete date"
                );
                checked_auto = true;
            } else if path.get_without_slash().contains("today") {
                match info.date {
                    Smart::Custom(Some(_)) => {}
                    other => panic!(
                        "expected datetime.today() to resolve to a concrete date in DocumentInfo, got {other:?}"
                    ),
                }
                checked_today = true;
            }
        }
        assert!(
            checked_auto && checked_today,
            "both documents must be found in the bundle"
        );
    }

    /// `rheo-metadata` is in scope at bundle-root (marrow) scope, not just
    /// each vertebra's own prelude, so a `.marrow.typ` can call it directly.
    #[test]
    fn rheo_metadata_is_callable_from_marrow_scope() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut source_overlay = HashMap::new();
        source_overlay.insert(
            "content/intro.typ".to_string(),
            "#set document(title: [Intro Title])\n= Intro\n".to_string(),
        );

        // Hand-built the same shape `VirtualSpine::vertebra_injections` produces
        // for an `OnePerVertebra` layout: `rheo-metadata` + `rheo-context()` as
        // the prelude, the metadata beacon as the epilogue.
        let mut rheo_context = HashMap::new();
        rheo_context.insert(
            "content/intro.typ".to_string(),
            VertebraInjection {
                prelude: format!(
                    "{}\n\n{}\n\n",
                    TypstStmt::MetadataHelper,
                    TypstStmt::ContextBinding {
                        handle: "intro".to_string()
                    }
                ),
                epilogue: format!(
                    "\n{}\n",
                    TypstStmt::MetadataBeacon {
                        handle: "intro".to_string()
                    }
                ),
            },
        );

        // Main: one vertebra document (so its beacon epilogue is emitted),
        // followed by marrow-scope Typst calling `rheo-metadata` directly.
        let main = r#"
#document("intro.html", format: "html")[
  #include "content/intro.typ"
]

#context { assert(rheo-metadata("intro").title != none) }
"#
        .to_string();

        let world = RheoWorld::new_for_bundle(
            root,
            main,
            source_overlay,
            rheo_context,
            None,
            Some("html"),
            vec![],
        )
        .unwrap();

        world
            .compile_bundle()
            .expect("rheo-metadata must be callable from marrow scope");
    }

    /// The companion `rheo-metadata-all()` — marrow-scope only (never in the
    /// per-vertebra prelude) — returns one entry per `spine-flat` vertebra.
    #[test]
    fn rheo_metadata_all_returns_one_entry_per_vertebra() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let global_context = TypstLiteral::Dict(vec![(
            "spine-flat".to_string(),
            TypstLiteral::Array(vec![
                TypstLiteral::Dict(vec![
                    ("handle".to_string(), TypstLiteral::str("intro")),
                    ("path".to_string(), TypstLiteral::str("content/intro.typ")),
                    ("title".to_string(), TypstLiteral::str("Intro")),
                ]),
                TypstLiteral::Dict(vec![
                    ("handle".to_string(), TypstLiteral::str("second")),
                    ("path".to_string(), TypstLiteral::str("content/second.typ")),
                    ("title".to_string(), TypstLiteral::str("Second")),
                ]),
            ]),
        )]);

        let main = "#context { assert(rheo-metadata-all().len() == 2) }\n".to_string();

        let world = RheoWorld::new_for_bundle(
            root,
            main,
            HashMap::new(),
            HashMap::new(),
            Some(global_context),
            Some("html"),
            vec![],
        )
        .unwrap();

        world
            .compile_bundle()
            .expect("rheo-metadata-all() must return one entry per spine-flat vertebra");
    }
}

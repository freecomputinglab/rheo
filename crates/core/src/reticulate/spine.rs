mod scan;
mod section;
mod serialize;
mod tree;

pub use serialize::FormatContext;
use tree::{SpineNode, tree_indices};

use crate::parser;
use crate::reticulate::bundle_source::BundleSource;
use crate::reticulate::document_meta::DocumentTitle;
use crate::reticulate::handle::Handle;
use crate::synth::typst_source::{TypstBlock, TypstStmt};
use crate::util::path::to_forward_slash;
use crate::{MARROW_FILE, RESERVED_META_LABEL_PREFIX, Result, RheoError};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;
use typst::syntax::Source;

// ── Directory scan: SpineScan ────────────────────────────────────────────────

/// A content directory scanned into a structured spine tree.
///
/// Built via [`SpineScan::run`], which walks the directory on disk.
#[derive(Debug)]
pub struct SpineScan {
    /// Ordered flat file list in pre-order (feeds `VirtualSpine::build` later).
    pub files: Vec<PathBuf>,
    /// Structured tree; `node.vertebra()` indexes into `files` (== pre-order position).
    pub tree: Vec<SpineNode>,
}

impl SpineScan {
    /// Recursively scan `content_dir`, producing the structured spine tree and
    /// its matching ordered flat file list.
    ///
    /// `exclude` is a list of glob patterns matched against each candidate
    /// path relative to `content_dir` (forward-slash separated); matching
    /// files or directories are dropped entirely.
    pub fn run(content_dir: &Path, exclude: &[String]) -> Result<SpineScan> {
        Self::run_with_marrow(content_dir, exclude, MARROW_FILE)
    }

    /// As [`Self::run`], but with the project's configured marrow filename —
    /// that file is inlined at bundle root rather than compiled as a vertebra,
    /// so the scan must skip it whatever it is called.
    pub fn run_with_marrow(
        content_dir: &Path,
        exclude: &[String],
        marrow_file: &str,
    ) -> Result<SpineScan> {
        // The marrow file is inlined at bundle root, never compiled as a
        // vertebra, so the scan must not see it. Injected as an escaped literal
        // pattern rather than read from the user's `exclude`, which stays their
        // own knob; `literal_separator` keeps it matching only at the top level,
        // where marrow is actually read from.
        let mut exclude_patterns = exclude.to_vec();
        exclude_patterns.push(globset::escape(marrow_file));
        let exclude_set = Self::build_exclude_set(&exclude_patterns)?;

        let mut files = Vec::new();
        let tree = Self::scan_dir(content_dir, content_dir, &exclude_set, &mut files)?;

        if files.is_empty() {
            return Err(RheoError::project_config("need at least one .typ file"));
        }

        // Only the marrow file directly under content_dir is inlined at the
        // bundle root. A same-named file deeper in the tree is compiled as an
        // ordinary vertebra — visible, but almost certainly not what the author
        // meant, and its leading dot is sanitized into a page named _marrow, so
        // say so rather than letting it look like marrow that silently did
        // nothing.
        for file in &files {
            if file.file_name().and_then(|n| n.to_str()) == Some(marrow_file) {
                let shown = file.strip_prefix(content_dir).unwrap_or(file);
                warn!(
                    path = %to_forward_slash(shown),
                    "marrow is only read from the top level of the content directory; \
                     this nested file is being compiled as an ordinary page"
                );
            }
        }

        debug_assert!(
            {
                let indices = tree_indices(&tree);
                let unique: HashSet<usize> = indices.iter().copied().collect();
                indices.len() == unique.len() && indices.iter().all(|&i| i < files.len())
            },
            "spine scan tree indices must be unique and in range"
        );

        Ok(SpineScan { files, tree })
    }

    /// Build a flat spine (no nesting) from an explicit, ordered file list.
    ///
    /// Each file becomes a top-level leaf whose segment is its full `:`-joined
    /// disk-path handle relative to `content_dir` (e.g. `a/notes.typ` →
    /// `a:notes`), preserving the given order. Used for single-file projects and
    /// wherever an explicit ordering is supplied rather than a directory scan.
    pub fn flat(files: &[PathBuf], content_dir: &Path) -> SpineScan {
        let tree = files
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let stem =
                    to_forward_slash(&f.strip_prefix(content_dir).unwrap_or(f).with_extension(""));
                let segment = stem
                    .split('/')
                    .map(Handle::sanitize_segment)
                    .collect::<Vec<_>>()
                    .join(":");
                SpineNode::leaf(segment, i)
            })
            .collect();
        SpineScan {
            files: files.to_vec(),
            tree,
        }
    }

    /// The handle each scanned file gets from its position in the tree: the
    /// `:`-joined path of ancestor segments down to it, indexed like
    /// [`SpineScan::files`]. Group nodes (a directory with no landing page, a
    /// `[[spine.section]]`) prefix their descendants without claiming a slot.
    ///
    /// Derivation is format-independent — no output layout, no extension — so a
    /// caller that only wants handles (`rheo migrate` resolving a `.typ` link
    /// target) asks for them without naming a format.
    pub fn handles(&self) -> Vec<Handle> {
        let mut handles: Vec<Handle> = vec![Handle::default(); self.files.len()];
        for node in &self.tree {
            node.visit("", &mut |path, node| {
                if let Some(&i) = node.vertebra()
                    && let Some(slot) = handles.get_mut(i)
                {
                    *slot = Handle::new(path.to_string());
                }
            });
        }
        handles
    }

    /// The same handles, keyed by each file's canonical path — the direction a
    /// caller resolving an on-disk path into a handle needs.
    pub fn handles_by_path(&self) -> HashMap<PathBuf, Handle> {
        self.files
            .iter()
            .zip(self.handles())
            .map(|(file, handle)| {
                let path =
                    crate::util::path::canonicalize_path(file).unwrap_or_else(|_| file.clone());
                (path, handle)
            })
            .collect()
    }
}

// ── Bundle spine: VirtualSpine, Vertebra, SpineLayout ────────────────────────

/// How a spine is compiled into output files under the bundle path.
pub enum SpineLayout {
    /// One output file per vertebra (e.g. HTML: "intro.html", "closing.html").
    OnePerVertebra { ext: String, format: String },
    /// All vertebrae in one combined output (e.g. PDF: "doc.pdf").
    SingleCombined { output_name: String, format: String },
}

/// Resolved metadata for one vertebra in a bundle compile.
pub struct Vertebra {
    /// Path relative to the project root, forward-slash separated (for `#include`).
    pub rel_path: String,
    /// Output path key in VirtualFs (e.g. "intro.html").
    pub output_path: String,
    /// Primary synthesized cross-vertebra handle label (e.g. "intro" or "chapters:intro").
    pub handle: Handle,
    /// Additional handle aliases; always includes the `<stem.typ>` escape form.
    pub extra_handles: Vec<String>,
    /// Whether the canonical `<handle>` label should be emitted as a bundle anchor.
    /// False when a user-authored label already occupies the canonical name.
    pub emit_handle: bool,
    /// Document title for `#document title:` and `@handle` display text.
    pub title: String,
    /// The vertebra's raw source text, retained for the Mould stage.
    pub source: String,
}

impl Vertebra {
    /// Return `true` if this vertebra's output path collides with `other`.
    pub fn collides_with(&self, other: &Vertebra) -> bool {
        self.output_path == other.output_path
    }
}

/// The Typst source injected around one vertebra's own body: a `prelude`
/// prepended before it, an `epilogue` appended after it. See
/// [`VirtualSpine::vertebra_injections`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VertebraInjection {
    /// Text prepended before the vertebra's own source (the `rheo-metadata`
    /// helper and the `rheo-context()` binding).
    pub prelude: String,
    /// Text appended after the vertebra's own source (the metadata beacon,
    /// when emitted for this layout — empty otherwise).
    pub epilogue: String,
}

/// A resolved spine ready for bundle compilation.
///
/// Constructed via `VirtualSpine::build`; call `source()` to get the synthesized
/// Typst source that drives `RheoWorld::compile_bundle`.
pub struct VirtualSpine {
    pub vertebrae: Vec<Vertebra>,
    pub layout: SpineLayout,
    /// Structural overlay over `vertebrae`; a flat one-level tree today, but the
    /// foundation for arbitrary nesting later.
    pub tree: Vec<SpineNode>,
    /// Resolved title for the combined output document, when configured
    /// (per-format `[plugin.spine] title`, else the global `[spine] title`).
    /// Not set by `build()` itself — callers resolve it from config and apply
    /// it with [`Self::with_title`], since `VirtualSpine` is built from a pure
    /// directory scan with no config access of its own.
    pub title: Option<String>,
    /// Marrow: raw Typst blobs emitted at bundle root, outside every document,
    /// so they can mint extra output files. Resolved by callers (the author's
    /// `.marrow.typ`, later also package-declared contributions) and applied
    /// with [`Self::with_marrow`], for the same no-config-access reason as
    /// `title`.
    pub marrow: Vec<String>,
    /// Marrow spliced BEFORE every document instead of after, so a `#show`/`#set`
    /// rule in it reaches pre-existing vertebrae (introspection is bundle-wide,
    /// not sequential). Global-by-default and powerful — opt-in only, applied
    /// with [`Self::with_marrow_prologue`].
    pub marrow_prologue: Vec<String>,
}

impl VirtualSpine {
    /// Attach a resolved spine title, builder-style.
    pub fn with_title(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }

    /// Attach marrow contributions spliced after every document (today's
    /// default position), builder-style.
    pub fn with_marrow(mut self, marrow: Vec<String>) -> Self {
        self.marrow = marrow;
        self
    }

    /// Attach marrow contributions spliced before every document, builder-style.
    pub fn with_marrow_prologue(mut self, marrow: Vec<String>) -> Self {
        self.marrow_prologue = marrow;
        self
    }

    /// Pre-order walk of `self.tree`, yielding `&Vertebra` for every node that
    /// points at one, in the same order as `self.vertebrae` for the trivial flat
    /// tree built by `build()`. Group nodes with `vertebra: None` still recurse
    /// into their children. A stale index is silently skipped, never panics.
    pub fn flat_vertebrae(&self) -> Vec<&Vertebra> {
        tree_indices(&self.tree)
            .into_iter()
            .filter_map(|i| self.vertebrae.get(i))
            .collect()
    }

    /// The vertebra a tree node points at, or `None` for a group node or a
    /// stale index. Never panics — looks up via `.get`.
    pub fn vertebra_of(&self, node: &SpineNode) -> Option<&Vertebra> {
        node.vertebra().and_then(|&i| self.vertebrae.get(i))
    }

    /// Resolve a list of spine files into a `VirtualSpine` with computed handles,
    /// output paths, and titles.
    ///
    /// `content_dir` is the project content root; stems are computed relative to it
    /// so `content/chapters/intro.typ` yields handle `intro` (or `chapters:intro`
    /// on a cross-directory stem collision). Pass `project_root` for `#include` paths.
    pub fn build(scan: SpineScan, project_root: &Path, layout: SpineLayout) -> Result<Self> {
        let handles = scan.handles();
        let SpineScan { files, tree } = scan;

        // First pass: parse each file, compute handles, collect user labels.
        struct FileInfo {
            file: PathBuf,
            handle: Handle,
            escape: String,
            output_path: String,
            rel_path: String,
            title: String,
            source: String,
        }

        // Union of all user-authored labels across the spine, as they land in
        // the bundle. Used by the page-handle checks below (canonical-skip and
        // escape-collision) to detect a user label occupying a synthesized
        // handle name.
        let mut user_labels: HashSet<String> = HashSet::new();

        let file_infos: Result<Vec<FileInfo>> = files
            .iter()
            .zip(handles.iter())
            .map(|(file, handle)| {
                let handle = handle.clone();
                let escape = handle.escape();

                let output_path = match &layout {
                    SpineLayout::OnePerVertebra { ext, .. } => handle.output_path(ext),
                    SpineLayout::SingleCombined { output_name, .. } => output_name.clone(),
                };

                // The scan already proved this path exists and ends in `.typ`,
                // so a read failure here is a real fault, not an absence.
                let source = fs::read_to_string(file).map_err(|e| {
                    RheoError::io(e, format!("reading spine file '{}'", file.display()))
                })?;
                let stem = file
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let rel_path = to_forward_slash(file.strip_prefix(project_root).unwrap_or(file));

                let source_obj = Source::detached(&source);
                let extracted = parser::extract_nodes(&source_obj);
                let sites = extracted.labels;

                // Title is purely path-derived (filename, title-cased). The
                // real authored title (e.g. via `#set document(title: ...)`,
                // including through an imported `#show:` template) is resolved
                // by Typst itself and read post-compile from `DocumentInfo`
                // (see `crate::plugins::document_meta::DocumentMeta` and
                // `crate::build::flatten_bundle_outputs`); this path-derived
                // value is only a pre-compile placeholder (spine ordering,
                // `@handle` display text before the bundle compiles, etc.).
                let title = DocumentTitle::to_readable_name(&stem);

                // The `rheo-meta:` namespace is reserved for the synthesized
                // per-vertebra metadata beacon (`TypstStmt::MetadataBeacon`).
                // Unlike the escape-collision check below (which only fires on
                // an actual collision), any authored label squatting on this
                // prefix is always a hard error — there is no useful silent
                // fallback, and it doesn't matter whether the label happens to
                // match a real beacon handle in this project.
                if let Some(offending) = sites
                    .definitions
                    .iter()
                    .find(|d| d.name.starts_with(RESERVED_META_LABEL_PREFIX))
                {
                    return Err(RheoError::invalid_data(format!(
                        "{}: label <{}> uses the reserved `{}` prefix, which rheo uses internally for per-vertebra document metadata",
                        file.display(),
                        offending.name,
                        RESERVED_META_LABEL_PREFIX
                    )));
                }

                user_labels.extend(sites.definitions.iter().map(|d| d.name.clone()));

                Ok(FileInfo {
                    file: file.clone(),
                    handle,
                    escape,
                    output_path,
                    rel_path,
                    title,
                    source,
                })
            })
            .collect();
        let file_infos = file_infos?;
        let all_user_labels = user_labels;

        // Second pass: assign emit_handle and check escape uniqueness.
        let mut seen_canonicals: HashSet<Handle> = HashSet::new();
        let mut seen_escapes: HashSet<String> = HashSet::new();

        let vertebrae: Result<Vec<Vertebra>> = file_infos
            .into_iter()
            .map(|fi| {
                // Canonical: skip if claimed by user or already emitted.
                let emit_handle = !all_user_labels.contains(fi.handle.as_str())
                    && !seen_canonicals.contains(&fi.handle);
                seen_canonicals.insert(fi.handle.clone());

                // Escape: must be unique — error on collision.
                if all_user_labels.contains(&fi.escape) || seen_escapes.contains(&fi.escape) {
                    return Err(RheoError::invalid_data(format!(
                        "{}: escape label <{}> collides with another label in the project",
                        fi.file.display(),
                        fi.escape
                    )));
                }
                seen_escapes.insert(fi.escape.clone());

                Ok(Vertebra {
                    rel_path: fi.rel_path,
                    output_path: fi.output_path,
                    handle: fi.handle,
                    extra_handles: vec![fi.escape],
                    emit_handle,
                    title: fi.title,
                    source: fi.source,
                })
            })
            .collect();

        let vertebrae = vertebrae?;

        debug_assert!(
            {
                let indices = tree_indices(&tree);
                let unique: HashSet<usize> = indices.iter().copied().collect();
                indices.len() == vertebrae.len() && unique.len() == indices.len()
            },
            "spine tree must reference every vertebra exactly once"
        );

        Ok(Self {
            vertebrae,
            layout,
            tree,
            title: None,
            marrow: Vec::new(),
            marrow_prologue: Vec::new(),
        })
    }

    /// Per-vertebra Typst injections, keyed by include path (`rel_path`): a
    /// `prelude` (prepended before the vertebra's own body) and an `epilogue`
    /// (appended after it).
    ///
    /// Each vertebra's `prelude` defines `rheo-metadata` (see
    /// [`TypstStmt::MetadataHelper`]) followed by the `rheo-context()`
    /// function, which composes this file's own `handle` and `metadata-of:
    /// rheo-metadata` with the format-global values (`spine`, `spine-flat`,
    /// `target`, `ext`) spread from `sys.inputs.rheo-context`. Only the
    /// per-file `handle` is baked here; the shared (potentially large) spine
    /// lives once in [`Self::global_context`], not duplicated per vertebra.
    /// `sys.inputs` reads need no `#context`, so authors read `rheo-context()`
    /// fields directly.
    ///
    /// Each vertebra's `epilogue` is its metadata beacon (see
    /// [`TypstStmt::MetadataBeacon`]) — but only for `OnePerVertebra` layouts.
    /// A `SingleCombined` (PDF) layout wraps every vertebra in one shared
    /// `#document(...)`, where a beacon would leak the preceding vertebra's
    /// `set document(...)` state into the next one (confirmed empirically in
    /// `docs/spikes/typst-native-metadata.md`, Q6), so `epilogue` is empty
    /// there; `rheo-metadata` is still defined (it just finds no beacon and
    /// returns `(:)`).
    pub fn vertebra_injections(&self) -> HashMap<String, VertebraInjection> {
        let emit_beacon = matches!(self.layout, SpineLayout::OnePerVertebra { .. });
        self.vertebrae
            .iter()
            .map(|v| {
                let prelude = format!(
                    "{}\n\n",
                    TypstBlock(vec![
                        TypstStmt::MetadataHelper,
                        TypstStmt::ContextBinding {
                            handle: v.handle.clone(),
                        },
                    ])
                );
                let epilogue = if emit_beacon {
                    let beacon = TypstStmt::MetadataBeacon {
                        handle: v.handle.clone(),
                    };
                    format!("\n{beacon}\n")
                } else {
                    String::new()
                };
                (v.rel_path.clone(), VertebraInjection { prelude, epilogue })
            })
            .collect()
    }

    /// Validate that no two vertebrae produce the same output path.
    ///
    /// Skipped for `SingleCombined` layouts where all vertebrae intentionally share
    /// the same output path (e.g. every vertebra produces "book.pdf").
    pub fn check_output_collisions(&self) -> Result<()> {
        if !matches!(self.layout, SpineLayout::OnePerVertebra { .. }) {
            return Ok(());
        }
        let mut seen: HashMap<&str, &Vertebra> = HashMap::new();
        for v in &self.vertebrae {
            if let Some(prev) = seen.insert(v.output_path.as_str(), v) {
                return Err(RheoError::project_config(format!(
                    "spine output path collision: '{}' produced by both '{}' and '{}'",
                    v.output_path, prev.rel_path, v.rel_path
                )));
            }
        }
        Ok(())
    }

    /// Synthesize the virtual main Typst source that drives `RheoWorld::compile_bundle`.
    ///
    /// For `OnePerVertebra` each vertebra becomes a `#document(output-path)[...]` containing
    /// a labeled `#figure` handle anchor followed by a real `#include`.
    /// For `SingleCombined` all vertebrae are wrapped in one `#document`, each with its
    /// own handle anchors emitted immediately before its `#include` so cross-references
    /// resolve to the correct location within the combined output.
    ///
    /// The handle anchor uses `#figure` rather than `#metadata` or a bare label.
    /// `#metadata(none) <label>` fails at compile time ("cannot reference metadata");
    /// `<label>` after a `#document` block labels the document element itself, which
    /// Typst also refuses to reference ("cannot reference document"). A labeled
    /// `#figure([title], kind: "rheo-handle", …)` is the only mechanism that allows
    /// cross-document `@handle` resolution. The corresponding `#show figure.where(kind:
    /// "rheo-handle"): none` in rheo.typ suppresses its rendering.
    pub fn source(&self) -> String {
        self.bundle_source().to_string()
    }

    /// Build the structured `BundleSource` representation of this spine.
    pub fn bundle_source(&self) -> BundleSource {
        use crate::reticulate::bundle_source::{BundleAnchor, BundleDocument, BundleSegment};

        // A vertebra's handle anchors: the canonical `<handle>` (when emitted) plus
        // the `<handle.typ>` escape aliases. Emitted before the vertebra's include so
        // cross-references resolve to the right location in the output.
        let segment_for = |v: &Vertebra| BundleSegment {
            anchors: v
                .emit_handle
                .then(|| v.handle.as_str())
                .into_iter()
                .chain(v.extra_handles.iter().map(String::as_str))
                .map(|label| BundleAnchor {
                    label: label.to_string(),
                    handle: v.handle.clone(),
                    title: v.title.clone(),
                })
                .collect(),
            include: v.rel_path.clone(),
        };

        let documents = match &self.layout {
            SpineLayout::OnePerVertebra { format, .. } => self
                .vertebrae
                .iter()
                .map(|v| BundleDocument {
                    output_path: v.output_path.clone(),
                    format: format.clone(),
                    title: v.title.clone(),
                    handle: v.handle.clone(),
                    segments: vec![segment_for(v)],
                })
                .collect(),
            SpineLayout::SingleCombined {
                output_name,
                format,
            } => {
                let title = self
                    .vertebrae
                    .first()
                    .map(|v| v.title.as_str())
                    .unwrap_or("Document")
                    .to_string();
                vec![BundleDocument {
                    output_path: output_name.clone(),
                    format: format.clone(),
                    title,
                    // Combined PDF is one document with no cross-vertebra link
                    // rule; the handle is unused.
                    handle: Handle::default(),
                    segments: self.vertebrae.iter().map(segment_for).collect(),
                }]
            }
        };

        let to_stmts = |texts: &[String]| {
            texts
                .iter()
                .map(|text| TypstStmt::Raw(text.clone()))
                .collect()
        };

        BundleSource {
            documents,
            marrow_prologue: to_stmts(&self.marrow_prologue),
            marrow: to_stmts(&self.marrow),
        }
    }
}

/// Test fixture shared by the [`scan`] and [`section`] submodules' own test
/// modules (both build spines from an ad hoc directory of empty `.typ`
/// files); private items of this module are visible to all of its
/// descendants, so no re-export is needed for them to reach it.
#[cfg(test)]
fn create_test_dir_with_files(files: &[&str]) -> tempfile::TempDir {
    let temp = tempfile::TempDir::new().unwrap();
    for file in files {
        let path = temp.path().join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, "").unwrap();
    }
    temp
}

/// Test fixture shared by [`scan`] and [`section`]: find a direct child node
/// by its `segment`, panicking with the sibling list on a miss.
#[cfg(test)]
fn find_node<'a>(nodes: &'a [SpineNode], segment: &str) -> &'a SpineNode {
    nodes
        .iter()
        .find(|n| n.segment == segment)
        .unwrap_or_else(|| {
            panic!(
                "node '{segment}' not found among {:?}",
                nodes.iter().map(|n| &n.segment).collect::<Vec<_>>()
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn unique_stems_get_bare_handle() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();
        fs::write(content.join("closing.typ"), "= Closing\n").unwrap();

        let files = vec![content.join("intro.typ"), content.join("closing.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        assert_eq!(spine.vertebrae[0].handle, "intro");
        assert_eq!(spine.vertebrae[0].extra_handles, vec!["intro.typ"]);
        assert_eq!(spine.vertebrae[0].output_path, "intro.html");
        assert_eq!(spine.vertebrae[1].handle, "closing");
        assert_eq!(spine.vertebrae[1].output_path, "closing.html");
    }

    #[test]
    fn unreadable_vertebra_errors_naming_the_path() {
        // A vertebra that cannot be read must fail the build loudly rather than
        // compile as a blank page.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        let missing = content.join("gone.typ");

        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let scan = SpineScan::flat(std::slice::from_ref(&missing), &content);
        let err = match VirtualSpine::build(scan, root, layout) {
            Err(e) => e,
            Ok(_) => panic!("an unreadable vertebra must not build"),
        };
        assert!(
            err.to_string().contains("gone.typ"),
            "error must name the unreadable file, got: {err}"
        );
    }

    #[test]
    fn nested_handle_maps_to_slash_output_path() {
        // The handle joins nesting with ':' (a valid Typst label char), but the
        // per-vertebra output_path is a real file path, so nested vertebrae must
        // land in on-disk subdirectories: handle "pages:about" → "pages/about.html".
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(content.join("pages")).unwrap();
        fs::write(content.join("pages").join("about.typ"), "= About\n").unwrap();

        let files = vec![content.join("pages").join("about.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        assert_eq!(spine.vertebrae[0].handle, "pages:about");
        assert_eq!(spine.vertebrae[0].output_path, "pages/about.html");
    }

    #[test]
    fn flat_accessor_matches_vertebrae_order() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();
        fs::write(content.join("closing.typ"), "= Closing\n").unwrap();

        let files = vec![content.join("intro.typ"), content.join("closing.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        let expected: Vec<&str> = spine.vertebrae.iter().map(|v| v.handle.as_str()).collect();
        let actual: Vec<&str> = spine
            .flat_vertebrae()
            .iter()
            .map(|v| v.handle.as_str())
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn vertebra_retains_source() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro <etal>\n\nSee @etal.\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();
        let v = &spine.vertebrae[0];

        // The raw source is retained for the Mould stage.
        assert!(v.source.contains("<etal>"));
    }

    #[test]
    fn vertebra_injection_prelude_is_composed_function_with_own_handle() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        let chapters = content.join("chapters");
        fs::create_dir_all(&chapters).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();
        fs::write(chapters.join("intro.typ"), "= Chapter\n").unwrap();

        let files = vec![content.join("intro.typ"), chapters.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        let injections = spine.vertebra_injections();
        // One injection per vertebra, keyed by include path.
        assert_eq!(injections.len(), 2);
        let root_injection = &injections["content/intro.typ"];
        let nested_injection = &injections["content/chapters/intro.typ"];

        // Each bakes only its OWN handle...
        assert!(root_injection.prelude.contains("handle: \"intro\""));
        assert!(
            nested_injection
                .prelude
                .contains("handle: \"chapters:intro\"")
        );
        for inj in [root_injection, nested_injection] {
            let p = &inj.prelude;
            // ...the rheo-metadata helper is defined ahead of rheo-context()...
            assert!(p.contains("#let rheo-metadata(handle) = "));
            assert!(
                p.find("rheo-metadata(handle)").unwrap()
                    < p.find("#let rheo-context() = ").unwrap()
            );
            // ...as a function that spreads the format-global values from
            // sys.inputs (composed, not baked), and carries metadata-of...
            assert!(p.contains("#let rheo-context() = "));
            assert!(p.contains("metadata-of: rheo-metadata"));
            assert!(p.contains("..sys.inputs.rheo-context"));
            // ...so the large spine is NOT duplicated into the per-file prelude.
            assert!(!p.contains("spine-flat"));
            assert!(!p.contains("path:"));
            // OnePerVertebra layouts get a beacon epilogue naming this vertebra.
            assert!(inj.epilogue.contains("#metadata("));
        }
        assert!(root_injection.epilogue.contains("<rheo-meta:intro>"));
        assert!(
            nested_injection
                .epilogue
                .contains("<rheo-meta:chapters:intro>")
        );
    }

    #[test]
    fn vertebra_injection_epilogue_empty_for_single_combined_layout() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("a.typ"), "= A\n").unwrap();

        let files = vec![content.join("a.typ")];
        let layout = SpineLayout::SingleCombined {
            output_name: "book.pdf".into(),
            format: "pdf".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        let injections = spine.vertebra_injections();
        let injection = &injections["content/a.typ"];
        // No beacon under combined PDF (Q6: it would leak into later vertebrae).
        assert_eq!(injection.epilogue, "");
        // The helper (and rheo-context's metadata-of field) are still defined,
        // so `(rheo-context().metadata-of)(...)` is always callable.
        assert!(injection.prelude.contains("#let rheo-metadata(handle) = "));
        assert!(injection.prelude.contains("metadata-of: rheo-metadata"));
    }

    #[test]
    fn nested_files_get_colon_qualified_handle() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        let a = content.join("a");
        let b = content.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("notes.typ"), "").unwrap();
        fs::write(b.join("notes.typ"), "").unwrap();

        let files = vec![a.join("notes.typ"), b.join("notes.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        assert_eq!(spine.vertebrae[0].handle, "a:notes");
        assert_eq!(spine.vertebrae[1].handle, "b:notes");
        assert!(
            spine.vertebrae[0]
                .extra_handles
                .contains(&"a:notes.typ".to_string())
        );
        assert_ne!(
            spine.vertebrae[0].output_path,
            spine.vertebrae[1].output_path
        );
    }

    #[test]
    fn virtual_main_html_shape() {
        let v = Vertebra {
            rel_path: "content/intro.typ".into(),
            output_path: "intro.html".into(),
            handle: "intro".into(),
            extra_handles: vec!["intro.typ".into()],
            emit_handle: true,
            title: "Introduction".into(),
            source: String::new(),
        };
        let spine = VirtualSpine {
            vertebrae: vec![v],
            layout: SpineLayout::OnePerVertebra {
                ext: "html".into(),
                format: "html".into(),
            },
            tree: Vec::new(),
            title: None,
            marrow: Vec::new(),
            marrow_prologue: Vec::new(),
        };
        let src = spine.source();
        assert!(src.contains("#document(\"intro.html\", format: \"html\""));
        // Per-document init hook: publishes the handle and (per-page) resets footnotes.
        assert!(src.contains("#rheo-page-init(\"intro\")"));
        assert!(src.contains("rheo-handle"));
        assert!(src.contains("[Introduction]"));
        assert!(src.contains("<intro>"));
        assert!(src.contains("<intro.typ>"));
        assert!(src.contains("#include \"content/intro.typ\""));
    }

    #[test]
    fn virtual_main_pdf_shape() {
        let spine = VirtualSpine {
            vertebrae: vec![
                Vertebra {
                    rel_path: "content/a.typ".into(),
                    output_path: "doc.pdf".into(),
                    handle: "a".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "A".into(),
                    source: String::new(),
                },
                Vertebra {
                    rel_path: "content/b.typ".into(),
                    output_path: "doc.pdf".into(),
                    handle: "b".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "B".into(),
                    source: String::new(),
                },
            ],
            layout: SpineLayout::SingleCombined {
                output_name: "doc.pdf".into(),
                format: "pdf".into(),
            },
            tree: Vec::new(),
            title: None,
            marrow: Vec::new(),
            marrow_prologue: Vec::new(),
        };
        let src = spine.source();
        assert!(src.contains("#document(\"doc.pdf\", format: \"pdf\""));
        // Combined PDF is one document with an empty handle; the hook still runs
        // (no `ext` at compile time -> the footnote reset inside it is skipped).
        assert!(src.contains("#rheo-page-init(\"\")"));
        assert!(src.contains("#include \"content/a.typ\""));
        assert!(src.contains("#include \"content/b.typ\""));
        // Synthesized handle anchors are now injected into the combined document so
        // cross-vertebra `@handle` references resolve.
        assert!(src.contains("rheo-handle"));
        assert!(src.contains("<a>"));
        assert!(src.contains("<b>"));
    }

    #[test]
    fn single_combined_collision_check_passes() {
        let spine = VirtualSpine {
            vertebrae: vec![
                Vertebra {
                    rel_path: "a.typ".into(),
                    output_path: "book.pdf".into(),
                    handle: "a".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "A".into(),
                    source: String::new(),
                },
                Vertebra {
                    rel_path: "b.typ".into(),
                    output_path: "book.pdf".into(),
                    handle: "b".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "B".into(),
                    source: String::new(),
                },
            ],
            layout: SpineLayout::SingleCombined {
                output_name: "book.pdf".into(),
                format: "pdf".into(),
            },
            tree: Vec::new(),
            title: None,
            marrow: Vec::new(),
            marrow_prologue: Vec::new(),
        };
        assert!(spine.check_output_collisions().is_ok());
    }

    #[test]
    fn canonical_skipped_when_user_label_conflicts() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // Source file hand-authors the same label as rheo would synthesize.
        fs::write(content.join("intro.typ"), "= Intro <intro>\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        // Without prefixing, the raw `<intro>` collides with the canonical handle.
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        assert_eq!(spine.vertebrae[0].handle, "intro");
        // Canonical was user-claimed → not emitted.
        assert!(!spine.vertebrae[0].emit_handle);
        // Escape label still present.
        assert!(
            spine.vertebrae[0]
                .extra_handles
                .contains(&"intro.typ".to_string())
        );
    }

    #[test]
    fn escape_label_collision_returns_error() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // intro.typ hand-authors <intro.typ>, which is the escape alias for intro.typ itself.
        fs::write(content.join("intro.typ"), "= Intro <intro.typ>\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let result = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout);
        match result {
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("intro.typ"), "error should name label: {msg}");
            }
            Ok(_) => panic!("expected escape collision error"),
        }
    }

    #[test]
    fn reserved_meta_label_prefix_returns_error() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // intro.typ hand-authors a label squatting on the reserved beacon namespace.
        fs::write(content.join("intro.typ"), "#let x = 1 <rheo-meta:intro>\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let result = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout);
        match result {
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("intro.typ") && msg.contains("rheo-meta:intro"),
                    "error should name both file and label: {msg}"
                );
            }
            Ok(_) => panic!("expected reserved meta-label prefix error"),
        }
    }

    #[test]
    fn no_reserved_meta_label_builds_unaffected() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro <intro-section>\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();
        assert_eq!(spine.vertebrae[0].handle, "intro");
    }

    /// Builds a one-vertebra spine (an `index.typ` under `root/content`),
    /// shared by the prologue/epilogue ordering tests below.
    fn build_single_vertebra_spine(root: &Path) -> VirtualSpine {
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("index.typ"), "= Index").unwrap();

        let scan = SpineScan::run(&content, &[]).unwrap();
        VirtualSpine::build(
            scan,
            root,
            SpineLayout::OnePerVertebra {
                ext: "html".into(),
                format: "html".into(),
            },
        )
        .unwrap()
    }

    /// Marrow statements are emitted after every `#document` block by default,
    /// at bundle root, where `document()`/`asset()` are legal — the position a
    /// project gets with no `marrow_prologue` config and a package gets by
    /// shipping `.marrow.typ`.
    #[test]
    fn bundle_source_emits_marrow_after_documents() {
        let tmp = TempDir::new().unwrap();
        let spine = build_single_vertebra_spine(tmp.path())
            .with_marrow(vec!["#asset(\"extra/hello.txt\", \"hi\")".to_string()]);

        let source = spine.bundle_source().to_string();
        let marrow_at = source
            .find("#asset(\"extra/hello.txt\", \"hi\")")
            .expect("marrow statement emitted");
        let last_document_at = source.rfind("#document(").expect("a document is emitted");
        assert!(
            marrow_at > last_document_at,
            "marrow must follow every document, got:\n{source}"
        );
    }

    /// Prologue marrow — opted into via `with_marrow_prologue` (the project's
    /// `marrow_prologue = true`, or a package's `.marrow-prologue.typ`) — is
    /// emitted before every `#document` block instead.
    #[test]
    fn bundle_source_emits_marrow_prologue_before_documents() {
        let tmp = TempDir::new().unwrap();
        let spine = build_single_vertebra_spine(tmp.path())
            .with_marrow_prologue(vec!["#asset(\"extra/hello.txt\", \"hi\")".to_string()]);

        let source = spine.bundle_source().to_string();
        let marrow_at = source
            .find("#asset(\"extra/hello.txt\", \"hi\")")
            .expect("marrow statement emitted");
        let first_document_at = source.find("#document(").expect("a document is emitted");
        assert!(
            marrow_at < first_document_at,
            "prologue marrow must precede every document, got:\n{source}"
        );
    }
}

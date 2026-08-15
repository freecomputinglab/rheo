use crate::config::SpineSection;
use crate::parser;
use crate::parser::RheoValue;
use crate::reticulate::bundle_source::BundleSource;
use crate::util::path::{sanitize_handle_segment, to_forward_slash};
use crate::util::pdf::DocumentTitle;
use crate::util::typst_literal::TypstLiteral;
use crate::util::typst_source::TypstStmt;
use crate::{MARROW_FILE, RESERVED_META_LABEL_PREFIX, Result, RheoError, TYP_EXT};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
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
    /// Structured tree; `node.vertebra` indexes into `files` (== pre-order position).
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
                let mut indices = Vec::new();
                for node in &tree {
                    node.collect_indices(&mut indices);
                }
                let unique: HashSet<usize> = indices.iter().copied().collect();
                indices.len() == unique.len() && indices.iter().all(|&i| i < files.len())
            },
            "spine scan tree indices must be unique and in range"
        );

        Ok(SpineScan { files, tree })
    }

    /// Compile exclude globs into a path-aware [`GlobSet`] (matched against
    /// content_dir-relative, forward-slashed paths). `literal_separator` keeps
    /// `*` from crossing `/` while `**` still descends, matching the documented
    /// exclude semantics.
    fn build_exclude_set(exclude: &[String]) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();
        for g in exclude {
            let glob = GlobBuilder::new(g)
                .literal_separator(true)
                .build()
                .map_err(|e| {
                    RheoError::project_config(format!("invalid exclude glob '{}': {}", g, e))
                })?;
            builder.add(glob);
        }
        builder
            .build()
            .map_err(|e| RheoError::project_config(format!("invalid exclude globs: {}", e)))
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
                    .map(sanitize_handle_segment)
                    .collect::<Vec<_>>()
                    .join(":");
                SpineNode {
                    segment,
                    title: None,
                    vertebra: Some(i),
                    children: Vec::new(),
                }
            })
            .collect();
        SpineScan {
            files: files.to_vec(),
            tree,
        }
    }

    /// Return `true` if `path` (relative to `content_dir`) matches any exclude glob.
    fn is_excluded(content_dir: &Path, path: &Path, exclude: &GlobSet) -> bool {
        let rel = path.strip_prefix(content_dir).unwrap_or(path);
        exclude.is_match(to_forward_slash(rel))
    }

    /// Scan one directory, recursing into subdirectories. Returns the child
    /// node list for `dir`; pushes discovered files into `files` in pre-order.
    fn scan_dir(
        content_dir: &Path,
        dir: &Path,
        exclude: &GlobSet,
        files: &mut Vec<PathBuf>,
    ) -> Result<Vec<SpineNode>> {
        let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)
            .map_err(|e| {
                RheoError::project_config(format!("failed to read dir '{}': {}", dir.display(), e))
            })?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        let mut nodes = Vec::new();

        for entry in entries {
            let path = entry.path();

            if Self::is_excluded(content_dir, &path, exclude) {
                continue;
            }

            if path.is_dir() {
                if let Some(node) = Self::scan_subdir(content_dir, &path, exclude, files)? {
                    nodes.push(node);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some(&TYP_EXT[1..]) {
                // Root-level index.typ is a normal leaf; only nested dirs treat
                // it as a landing page (handled in scan_subdir).
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                let idx = files.len();
                files.push(path.clone());
                nodes.push(SpineNode {
                    segment: sanitize_handle_segment(stem),
                    title: None,
                    vertebra: Some(idx),
                    children: Vec::new(),
                });
            }
        }

        Ok(nodes)
    }

    /// Scan a subdirectory, deciding whether it has a landing page (clickable
    /// node) or not (group node). Returns `None` if the subtree contains no
    /// `.typ` files after exclusion (pruned).
    fn scan_subdir(
        content_dir: &Path,
        dir: &Path,
        exclude: &GlobSet,
        files: &mut Vec<PathBuf>,
    ) -> Result<Option<SpineNode>> {
        let dirname = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)
            .map_err(|e| {
                RheoError::project_config(format!("failed to read dir '{}': {}", dir.display(), e))
            })?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        // Find the landing file: prefer index.typ, else <dirname>.typ.
        let index_name = format!("index{}", TYP_EXT);
        let named_name = format!("{}{}", dirname, TYP_EXT);

        let landing_path = entries
            .iter()
            .map(|e| e.path())
            .filter(|p| !Self::is_excluded(content_dir, p, exclude))
            .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(index_name.as_str()))
            .or_else(|| {
                entries
                    .iter()
                    .map(|e| e.path())
                    .filter(|p| !Self::is_excluded(content_dir, p, exclude))
                    .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(named_name.as_str()))
            });

        let (vertebra, title) = if let Some(landing) = &landing_path {
            let idx = files.len();
            files.push(landing.clone());
            (Some(idx), None)
        } else {
            (None, Some(Self::prettify(&dirname)))
        };

        // Recurse for children, skipping the landing file itself.
        let mut children = Vec::new();
        for entry in &entries {
            let path = entry.path();

            if Some(&path) == landing_path.as_ref() {
                continue;
            }
            if Self::is_excluded(content_dir, &path, exclude) {
                continue;
            }

            if path.is_dir() {
                if let Some(node) = Self::scan_subdir(content_dir, &path, exclude, files)? {
                    children.push(node);
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some(&TYP_EXT[1..]) {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                let idx = files.len();
                files.push(path.clone());
                children.push(SpineNode {
                    segment: sanitize_handle_segment(stem),
                    title: None,
                    vertebra: Some(idx),
                    children: Vec::new(),
                });
            }
        }

        if vertebra.is_none() && children.is_empty() {
            // Empty subtree after exclusion/pruning: drop the whole node.
            return Ok(None);
        }

        Ok(Some(SpineNode {
            segment: sanitize_handle_segment(&dirname),
            title,
            vertebra,
            children,
        }))
    }

    /// Derive a group title from a directory name: strip a leading numeric
    /// order prefix (e.g. `01-`, `1_`), replace `-`/`_` with spaces, and Title
    /// Case each word.
    fn prettify(dirname: &str) -> String {
        let digits_end = dirname.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
        let stripped = if digits_end > 0 && dirname[digits_end..].starts_with(['-', '_']) {
            &dirname[digits_end + 1..]
        } else {
            dirname
        };

        stripped
            .split(['-', '_'])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Apply `[[spine.section]]` virtual-directory layering (knob 2) to a scanned
    /// spine, returning a new [`SpineScan`] with the tree + flat file list rebuilt.
    ///
    /// Each section is a virtual directory: its `include` globs pull matching
    /// leaf files out of their scanned position and nest them under a group node
    /// named `name` (arbitrary depth via nested `section`). A file placed under
    /// section `guide` later resolves to handle `guide:<stem>`, exactly as if it
    /// lived in `content/guide/`. Only leaf files are movable; a directory
    /// landing page (`index.typ`) stays where it is. Returns `self` unchanged
    /// when there are no sections.
    pub fn apply_sections(
        self,
        content_dir: &Path,
        sections: &[SpineSection],
    ) -> Result<SpineScan> {
        if sections.is_empty() {
            return Ok(self);
        }
        Self::validate_sections(sections)?;

        let mut roots = Self::to_path_nodes(&self.tree, &self.files);

        // All movable (leaf) file paths currently in the tree.
        let mut leaves = Vec::new();
        Self::collect_leaf_files(&roots, &mut leaves);

        // Build virtual section nodes, claiming leaf files (each to the first
        // section, pre-order, whose include matches it).
        let mut claimed: HashSet<PathBuf> = HashSet::new();
        let virtual_nodes =
            Self::build_section_nodes(content_dir, sections, &leaves, &mut claimed)?;

        // Remove claimed leaves from the scanned tree, pruning emptied groups.
        Self::prune_claimed(&mut roots, &claimed);

        // Insert virtual dirs at top level; order top-level siblings by segment,
        // just as on-disk directories are ordered by name.
        roots.extend(virtual_nodes);
        roots.sort_by(|a, b| a.segment.cmp(&b.segment));

        // Re-index into SpineNode + flat file list (pre-order, parent before child).
        let mut files = Vec::new();
        let tree = Self::reindex(&roots, &mut files);

        if files.is_empty() {
            return Err(RheoError::project_config(
                "spine is empty after applying sections",
            ));
        }
        Ok(SpineScan { files, tree })
    }

    /// Validate section names recursively: each `name` must be a non-empty slug
    /// and sibling names must be unique (they behave like directory names).
    fn validate_sections(sections: &[SpineSection]) -> Result<()> {
        let mut seen: HashSet<&str> = HashSet::new();
        for s in sections {
            if sanitize_handle_segment(&s.name).is_empty() {
                return Err(RheoError::project_config(format!(
                    "spine section name '{}' is not a valid slug",
                    s.name
                )));
            }
            if !seen.insert(s.name.as_str()) {
                return Err(RheoError::project_config(format!(
                    "duplicate spine section name '{}'",
                    s.name
                )));
            }
            Self::validate_sections(&s.section)?;
        }
        Ok(())
    }

    /// Convert an indexed [`SpineNode`] tree into a path-carrying working tree.
    fn to_path_nodes(nodes: &[SpineNode], files: &[PathBuf]) -> Vec<PathNode> {
        nodes
            .iter()
            .map(|n| PathNode {
                segment: n.segment.clone(),
                title: n.title.clone(),
                file: n.vertebra.and_then(|i| files.get(i)).cloned(),
                children: Self::to_path_nodes(&n.children, files),
            })
            .collect()
    }

    /// Collect every movable leaf file path (a node with a file and no children).
    fn collect_leaf_files(nodes: &[PathNode], out: &mut Vec<PathBuf>) {
        for n in nodes {
            if n.children.is_empty()
                && let Some(f) = &n.file
            {
                out.push(f.clone());
            }
            Self::collect_leaf_files(&n.children, out);
        }
    }

    /// Build virtual-directory nodes from `sections`, claiming leaf files.
    fn build_section_nodes(
        content_dir: &Path,
        sections: &[SpineSection],
        leaves: &[PathBuf],
        claimed: &mut HashSet<PathBuf>,
    ) -> Result<Vec<PathNode>> {
        let mut result = Vec::new();
        for s in sections {
            let mut children = Vec::new();

            // Match this section's includes, in listed order; within one glob,
            // lexicographic. A file is claimed by the first section that matches.
            let mut matched: Vec<PathBuf> = Vec::new();
            for g in &s.include {
                let matcher = GlobBuilder::new(g)
                    .literal_separator(true)
                    .build()
                    .map_err(|e| {
                        RheoError::project_config(format!(
                            "invalid include glob '{}' in spine section '{}': {}",
                            g, s.name, e
                        ))
                    })?
                    .compile_matcher();
                let mut ms: Vec<PathBuf> = leaves
                    .iter()
                    .filter(|p| !claimed.contains(*p))
                    .filter(|p| {
                        let rel = p.strip_prefix(content_dir).unwrap_or(p);
                        matcher.is_match(to_forward_slash(rel))
                    })
                    .cloned()
                    .collect();
                ms.sort();
                for m in ms {
                    if claimed.insert(m.clone()) {
                        matched.push(m);
                    }
                }
            }
            if !s.include.is_empty() && matched.is_empty() {
                return Err(RheoError::project_config(format!(
                    "spine section '{}' include matched no files",
                    s.name
                )));
            }

            for p in matched {
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                children.push(PathNode {
                    segment: sanitize_handle_segment(stem),
                    title: None,
                    file: Some(p),
                    children: Vec::new(),
                });
            }

            // Nested virtual directories.
            let nested = Self::build_section_nodes(content_dir, &s.section, leaves, claimed)?;
            children.extend(nested);

            result.push(PathNode {
                segment: sanitize_handle_segment(&s.name),
                title: Some(s.title.clone().unwrap_or_else(|| Self::prettify(&s.name))),
                file: None,
                children,
            });
        }
        Ok(result)
    }

    /// Remove claimed leaf files from the working tree, dropping any group node
    /// left with neither a file nor children.
    fn prune_claimed(nodes: &mut Vec<PathNode>, claimed: &HashSet<PathBuf>) {
        nodes.retain_mut(|n| {
            Self::prune_claimed(&mut n.children, claimed);
            if n.children.is_empty()
                && let Some(f) = &n.file
                && claimed.contains(f)
            {
                return false;
            }
            !(n.file.is_none() && n.children.is_empty())
        });
    }

    /// Re-index a working tree into an indexed [`SpineNode`] tree, rebuilding the
    /// flat file list in pre-order (parent before children).
    fn reindex(nodes: &[PathNode], files: &mut Vec<PathBuf>) -> Vec<SpineNode> {
        nodes
            .iter()
            .map(|n| {
                let vertebra = n.file.as_ref().map(|f| {
                    let idx = files.len();
                    files.push(f.clone());
                    idx
                });
                SpineNode {
                    segment: n.segment.clone(),
                    title: n.title.clone(),
                    vertebra,
                    children: Self::reindex(&n.children, files),
                }
            })
            .collect()
    }
}

/// A working spine node carrying file PATHS (not indices), used while
/// transforming the scanned tree before re-indexing into [`SpineNode`].
struct PathNode {
    segment: String,
    title: Option<String>,
    file: Option<PathBuf>,
    children: Vec<PathNode>,
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
    pub handle: String,
    /// Additional handle aliases; always includes the `<stem.typ>` escape form.
    pub extra_handles: Vec<String>,
    /// Whether the canonical `<handle>` label should be emitted as a bundle anchor.
    /// False when a user-authored label already occupies the canonical name.
    pub emit_handle: bool,
    /// Document title for `#document title:` and `@handle` display text.
    pub title: String,
    /// Harvested `rheo-*` variables from this vertebra's source file.
    pub vars: std::collections::HashMap<String, RheoValue>,
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

/// One node in the structured spine. Mirrors directory / section nesting to
/// arbitrary depth as a structural overlay over the flat `vertebrae`.
#[derive(Debug)]
pub struct SpineNode {
    /// Handle segment contributed by this node (dir name, file stem, or section
    /// name). For the trivial flat tree this is the vertebra's full handle.
    pub segment: String,
    /// Explicit group title. `None` for a leaf — display title comes from the
    /// vertebra it points at.
    pub title: Option<String>,
    /// Index into `VirtualSpine.vertebrae` for this node's landing page, or
    /// `None` for a non-clickable group node.
    pub vertebra: Option<usize>,
    /// Child nodes, in order.
    pub children: Vec<SpineNode>,
}

impl SpineNode {
    /// Pre-order walk: push this node's vertebra index (if any) then recurse
    /// into children regardless of whether this node itself yielded one.
    fn collect_indices(&self, out: &mut Vec<usize>) {
        if let Some(i) = self.vertebra {
            out.push(i);
        }
        for child in &self.children {
            child.collect_indices(out);
        }
    }
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
}

impl VirtualSpine {
    /// Attach a resolved spine title, builder-style.
    pub fn with_title(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }

    /// Attach marrow contributions, builder-style.
    pub fn with_marrow(mut self, marrow: Vec<String>) -> Self {
        self.marrow = marrow;
        self
    }

    /// Pre-order walk of `self.tree`, yielding `&Vertebra` for every node that
    /// points at one, in the same order as `self.vertebrae` for the trivial flat
    /// tree built by `build()`. Group nodes with `vertebra: None` still recurse
    /// into their children. A stale index is silently skipped, never panics.
    pub fn flat_vertebrae(&self) -> Vec<&Vertebra> {
        let mut indices = Vec::new();
        for node in &self.tree {
            node.collect_indices(&mut indices);
        }
        indices
            .into_iter()
            .filter_map(|i| self.vertebrae.get(i))
            .collect()
    }

    /// The vertebra a tree node points at, or `None` for a group node or a
    /// stale index. Never panics — looks up via `.get`.
    pub fn vertebra_of(&self, node: &SpineNode) -> Option<&Vertebra> {
        node.vertebra.and_then(|i| self.vertebrae.get(i))
    }

    /// Fill `handles[i]` with the `:`-joined segment path from the tree root to
    /// the node whose `vertebra` is `Some(i)`, walking pre-order. Group nodes
    /// contribute their segment as a prefix to descendants without claiming a slot.
    fn assign_handles(nodes: &[SpineNode], prefix: &str, handles: &mut [String]) {
        for n in nodes {
            let seg = if prefix.is_empty() {
                n.segment.clone()
            } else {
                format!("{prefix}:{}", n.segment)
            };
            if let Some(i) = n.vertebra
                && let Some(slot) = handles.get_mut(i)
            {
                *slot = seg.clone();
            }
            Self::assign_handles(&n.children, &seg, handles);
        }
    }

    /// Resolve a list of spine files into a `VirtualSpine` with computed handles,
    /// output paths, and titles.
    ///
    /// `content_dir` is the project content root; stems are computed relative to it
    /// so `content/chapters/intro.typ` yields handle `intro` (or `chapters:intro`
    /// on a cross-directory stem collision). Pass `project_root` for `#include` paths.
    pub fn build(scan: SpineScan, project_root: &Path, layout: SpineLayout) -> Result<Self> {
        let SpineScan { files, tree } = scan;

        // Handle per file, derived from its position in the spine tree: the
        // ':'-joined path of ancestor segments down to the file. For a plain
        // directory scan this equals the disk path; a file pulled under a
        // `[[spine.section]]` gains that section's segment as a prefix.
        let mut handles: Vec<String> = vec![String::new(); files.len()];
        Self::assign_handles(&tree, "", &mut handles);

        // First pass: parse each file, compute handles, collect user labels.
        struct FileInfo {
            file: PathBuf,
            handle: String,
            escape: String,
            output_path: String,
            rel_path: String,
            title: String,
            vars: HashMap<String, RheoValue>,
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
                let escape = format!("{handle}.typ");

                let output_path = match &layout {
                    // The handle joins nesting with ':' (a valid Typst label
                    // char; '/' is not). output_path is a real file path, so
                    // translate those separators back to '/' — nested vertebrae
                    // land in on-disk subdirectories, not colon-flattened files.
                    SpineLayout::OnePerVertebra { ext, .. } => {
                        format!("{}.{ext}", handle.replace(':', "/"))
                    }
                    SpineLayout::SingleCombined { output_name, .. } => output_name.clone(),
                };

                let source = fs::read_to_string(file).unwrap_or_default();
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
                let mut vars = HashMap::new();
                for v in extracted.rheo_vars {
                    match v.value {
                        Some(value) => {
                            vars.insert(v.key, value);
                        }
                        None => {
                            return Err(RheoError::invalid_data(format!(
                                "{}:{}: rheo-{} must be a string or boolean",
                                file.display(),
                                v.line,
                                v.key
                            )));
                        }
                    }
                }

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
                    vars,
                    source,
                })
            })
            .collect();
        let file_infos = file_infos?;
        let all_user_labels = user_labels;

        // Second pass: assign emit_handle and check escape uniqueness.
        let mut seen_canonicals: HashSet<String> = HashSet::new();
        let mut seen_escapes: HashSet<String> = HashSet::new();

        let vertebrae: Result<Vec<Vertebra>> = file_infos
            .into_iter()
            .map(|fi| {
                // Canonical: skip if claimed by user or already emitted.
                let emit_handle =
                    !all_user_labels.contains(&fi.handle) && !seen_canonicals.contains(&fi.handle);
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
                    vars: fi.vars,
                    source: fi.source,
                })
            })
            .collect();

        let vertebrae = vertebrae?;

        debug_assert!(
            {
                let mut indices = Vec::new();
                for node in &tree {
                    node.collect_indices(&mut indices);
                }
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
                let helper = TypstStmt::MetadataHelper;
                let binding = TypstStmt::ContextBinding {
                    handle: v.handle.clone(),
                };
                let prelude = format!("{helper}\n\n{binding}\n\n");
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

    /// The file-independent `rheo-context` data exposed via `sys.inputs`.
    ///
    /// `sys.inputs` is global to the whole bundle compile, so it carries only the
    /// parts of `rheo-context` identical across vertebrae — `spine`/`spine-flat`.
    /// Packages read `sys.inputs.rheo-context` to detect a rheo build (and reach
    /// the shared spine) without referencing the per-file `#let rheo-context`,
    /// which additionally carries this file's `handle`.
    ///
    /// `target` and `ext` follow the same rule as [`Self::vertebra_injections`]:
    /// each field is added when `Some`, omitted for PDF (`None`). `ext` is the
    /// output file extension (e.g. `"html"`/`"xhtml"`) — the value `typ/rheo.typ`
    /// reads to build cross-vertebra link hrefs.
    ///
    /// `reset_footnotes` is the resolved per-format `reset-footnotes` toggle:
    /// unlike `target`/`ext` it is always present (a resolved bool, not an
    /// `Option`), and `typ/rheo.typ` ANDs it with the per-page `ext` gate before
    /// resetting the footnote counter (so it only ever takes effect for HTML/EPUB).
    pub fn global_context(
        &self,
        target: Option<&str>,
        ext: Option<&str>,
        reset_footnotes: bool,
    ) -> TypstLiteral {
        let mut fields = vec![
            ("spine".to_string(), self.spine_tree()),
            ("spine-flat".to_string(), self.spine_flat()),
        ];
        if let Some(t) = target {
            fields.push(("target".to_string(), TypstLiteral::str(t)));
        }
        if let Some(e) = ext {
            fields.push(("ext".to_string(), TypstLiteral::str(e)));
        }
        fields.push((
            "reset-footnotes".to_string(),
            TypstLiteral::bool(reset_footnotes),
        ));
        TypstLiteral::Dict(fields)
    }

    /// The structured spine tree as a [`TypstLiteral`] array of recursive node
    /// dicts. See [`Self::node_literal`] for the node key set.
    fn spine_tree(&self) -> TypstLiteral {
        TypstLiteral::Array(self.tree.iter().map(|n| self.node_literal(n)).collect())
    }

    /// Serialize one [`SpineNode`] (and its descendants) to its `spine` dict
    /// shape: `title`/`handle`/`path`/`children`. Per-vertebra metadata is no
    /// longer carried here — read it live via `rheo-context().metadata-of`
    /// (see [`TypstStmt::MetadataHelper`]) instead.
    fn node_literal(&self, node: &SpineNode) -> TypstLiteral {
        let (handle, path, title) = match node.vertebra.and_then(|i| self.vertebrae.get(i)) {
            Some(v) => (
                TypstLiteral::str(v.handle.as_str()),
                TypstLiteral::str(v.rel_path.as_str()),
                TypstLiteral::str(v.title.as_str()),
            ),
            None => (
                TypstLiteral::None,
                TypstLiteral::None,
                TypstLiteral::str(node.title.as_deref().unwrap_or(node.segment.as_str())),
            ),
        };
        let children =
            TypstLiteral::Array(node.children.iter().map(|c| self.node_literal(c)).collect());
        TypstLiteral::Dict(vec![
            ("title".to_string(), title),
            ("handle".to_string(), handle),
            ("path".to_string(), path),
            ("children".to_string(), children),
        ])
    }

    /// The flat spine as a [`TypstLiteral`] array-of-dictionaries, in the same
    /// pre-order as [`Self::flat_vertebrae`]: one entry per clickable vertebra
    /// (group nodes excluded) with `handle`, `path`, and `title`.
    fn spine_flat(&self) -> TypstLiteral {
        TypstLiteral::Array(
            self.flat_vertebrae()
                .into_iter()
                .map(|v| {
                    TypstLiteral::Dict(vec![
                        ("handle".to_string(), TypstLiteral::str(v.handle.as_str())),
                        ("path".to_string(), TypstLiteral::str(v.rel_path.as_str())),
                        ("title".to_string(), TypstLiteral::str(v.title.as_str())),
                    ])
                })
                .collect(),
        )
    }

    /// Validate that no two vertebrae produce the same output path.
    ///
    /// Skipped for `SingleCombined` layouts where all vertebrae intentionally share
    /// the same output path (e.g. every vertebra produces "book.pdf").
    pub fn check_output_collisions(&self) -> Result<()> {
        if !matches!(self.layout, SpineLayout::OnePerVertebra { .. }) {
            return Ok(());
        }
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for (i, v) in self.vertebrae.iter().enumerate() {
            if let Some(prev) = seen.insert(v.output_path.as_str(), i) {
                return Err(RheoError::project_config(format!(
                    "spine output path collision: '{}' produced by vertebra {} and {}",
                    v.output_path, prev, i
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
                .then_some(&v.handle)
                .into_iter()
                .chain(v.extra_handles.iter())
                .map(|label| BundleAnchor {
                    label: label.clone(),
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
                    handle: String::new(),
                    segments: self.vertebrae.iter().map(segment_for).collect(),
                }]
            }
        };

        BundleSource {
            documents,
            marrow: self
                .marrow
                .iter()
                .map(|text| TypstStmt::Raw(text.clone()))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir_with_files(files: &[&str]) -> TempDir {
        let temp = TempDir::new().unwrap();
        for file in files {
            let path = temp.path().join(file);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, "").unwrap();
        }
        temp
    }

    // ── VirtualSpine tests ──────────────────────────────────────────────────

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
    fn spine_tree_nests_group_nodes_with_none_handle_and_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        let chapters = content.join("chapters");
        fs::create_dir_all(&chapters).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();
        fs::write(chapters.join("one.typ"), "= One\n").unwrap();

        let scan = SpineScan::run(&content, &[]).unwrap();
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(scan, root, layout).unwrap();

        let tree = spine.spine_tree().serialize();
        // Root leaf carries its own handle/path/title.
        assert!(tree.contains("handle: \"intro\""));
        assert!(tree.contains("path: \"content/intro.typ\""));
        // The `chapters` directory has no landing page: a group node with
        // handle/path `none` and its own title, nesting `one` as a child.
        assert!(tree.contains("handle: none"));
        assert!(tree.contains("path: none"));
        assert!(tree.contains("title: \"Chapters\""));
        assert!(tree.contains("children:"));
        assert!(tree.contains("handle: \"chapters:one\""));

        // spine-flat only lists clickable vertebrae, in pre-order.
        let flat = spine.spine_flat().serialize();
        assert!(flat.contains("handle: \"intro\""));
        assert!(flat.contains("handle: \"chapters:one\""));
        assert!(!flat.contains("title: \"Chapters\""));
    }

    #[test]
    fn spine_no_longer_exposes_a_metadata_key_on_entries() {
        // Superseded by the Typst-native metadata-beacon mechanism
        // (`rheo-context().metadata-of`, docs/spikes/typst-native-metadata.md):
        // the Rust-parsed `metadata` key is no longer serialized into either
        // the spine tree or spine-flat entries, and `#set document(...)` is no
        // longer statically scanned at all — `Vertebra.title` is purely
        // path-derived, so this vertebra's spine-entry title is "Post" (from
        // the filename), not the `#set document(title: ...)` value.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // A post whose `#set document(...)` carries keywords (tags) and an author.
        fs::write(
            content.join("post.typ"),
            "#set document(title: [My Post], keywords: (\"DiH\",), author: \"Jane\")\n= Body\n",
        )
        .unwrap();
        // A page with no `#set document(...)`.
        fs::write(content.join("bare.typ"), "= Bare\n").unwrap();

        let scan = SpineScan::run(&content, &[]).unwrap();
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(scan, root, layout).unwrap();

        for serialized in [
            spine.spine_tree().serialize(),
            spine.spine_flat().serialize(),
        ] {
            assert!(
                !serialized.contains("metadata:"),
                "metadata key should no longer be serialized: {serialized}"
            );
            // The other spine entry fields remain.
            assert!(serialized.contains("title: \"Post\""));
            assert!(serialized.contains("handle: \"post\""));
        }
    }

    #[test]
    fn rheo_context_target_and_ext_present_when_some_absent_when_none() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        // target/ext live on the global context (sys.inputs); the per-file
        // prelude only spreads them in, so they are asserted on global_context.
        let global_html = spine
            .global_context(Some("html"), Some("html"), true)
            .serialize();
        assert!(global_html.contains("target: \"html\""));
        assert!(global_html.contains("ext: \"html\""));
        // The resolved reset-footnotes toggle is always present (unlike target/ext).
        assert!(global_html.contains("reset-footnotes: true"));

        // Epub keeps `target` "epub" but `ext` "xhtml"; a false toggle is threaded through.
        let global_epub = spine
            .global_context(Some("epub"), Some("xhtml"), false)
            .serialize();
        assert!(global_epub.contains("target: \"epub\""));
        assert!(global_epub.contains("ext: \"xhtml\""));
        assert!(global_epub.contains("reset-footnotes: false"));

        // None (PDF) -> no `target` or `ext` field, but reset-footnotes is still present.
        let global_without = spine.global_context(None, None, true).serialize();
        assert!(!global_without.contains("target:"));
        assert!(!global_without.contains("ext:"));
        assert!(global_without.contains("reset-footnotes: true"));
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
            vars: HashMap::new(),
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
                    vars: HashMap::new(),
                    source: String::new(),
                },
                Vertebra {
                    rel_path: "content/b.typ".into(),
                    output_path: "doc.pdf".into(),
                    handle: "b".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "B".into(),
                    vars: HashMap::new(),
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
                    vars: Default::default(),
                    source: String::new(),
                },
                Vertebra {
                    rel_path: "b.typ".into(),
                    output_path: "book.pdf".into(),
                    handle: "b".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "B".into(),
                    vars: Default::default(),
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

    // ── SpineScan tests ─────────────────────────────────────────────────────

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

    #[test]
    fn scan_nested_tree_with_landing_pages() {
        let temp = create_test_dir_with_files(&[
            "index.typ",
            "intro.typ",
            "guide/index.typ",
            "guide/a.typ",
            "guide/b.typ",
            "guide/deep/x.typ",
        ]);

        let result = SpineScan::run(temp.path(), &[]).unwrap();
        assert_eq!(result.files.len(), 6);

        let guide = find_node(&result.tree, "guide");
        assert!(guide.vertebra.is_some());
        assert_eq!(guide.segment, "guide");

        let a = find_node(&guide.children, "a");
        assert!(a.vertebra.is_some());
        let _b = find_node(&guide.children, "b");

        let deep = find_node(&guide.children, "deep");
        assert!(deep.vertebra.is_none());
        let x = find_node(&deep.children, "x");
        assert!(x.vertebra.is_some());
    }

    #[test]
    fn scan_dir_without_index_is_group_node() {
        let temp = create_test_dir_with_files(&["extras/note.typ"]);
        let result = SpineScan::run(temp.path(), &[]).unwrap();

        let extras = find_node(&result.tree, "extras");
        assert!(extras.vertebra.is_none());
        assert_eq!(extras.title, Some("Extras".to_string()));
    }

    /// Marrow is emitted at the bundle root, outside every document, so the
    /// scan must never turn it into a vertebra of its own.
    #[test]
    fn scan_skips_marrow_file() {
        let temp = create_test_dir_with_files(&["index.typ", ".marrow.typ"]);
        let result = SpineScan::run(temp.path(), &[]).unwrap();

        assert_eq!(result.files.len(), 1, "only index.typ is a vertebra");
        assert!(
            result
                .files
                .iter()
                .all(|p| p.file_name().unwrap() != MARROW_FILE),
            ".marrow.typ must not be scanned as a vertebra"
        );
    }

    /// Only the marrow file directly under `content_dir` is special — that is
    /// the one rheo reads and inlines. A same-named file in a subdirectory is
    /// an ordinary vertebra, so it stays visible rather than silently vanishing
    /// from both the spine and the bundle root.
    #[test]
    fn scan_keeps_a_nested_marrow_named_file_as_a_vertebra() {
        let temp = create_test_dir_with_files(&["index.typ", ".marrow.typ", "sub/.marrow.typ"]);
        let result = SpineScan::run(temp.path(), &[]).unwrap();

        assert_eq!(
            result.files.len(),
            2,
            "expected index.typ and sub/.marrow.typ, got {:?}",
            result.files
        );
        assert!(
            result.files.iter().any(|p| p.ends_with("sub/.marrow.typ")),
            "a nested marrow-named file must remain a vertebra"
        );
        assert!(
            !result
                .files
                .iter()
                .any(|p| p.ends_with("index.typ") && p.parent().unwrap().ends_with("sub")),
            "sanity: no stray nesting"
        );
    }

    /// The marrow filename is configurable, so the scan must skip whatever the
    /// project named it — and treat the default name as an ordinary vertebra
    /// once it no longer is the marrow.
    #[test]
    fn scan_skips_the_configured_marrow_file() {
        let temp = create_test_dir_with_files(&["index.typ", "bundle-root.typ"]);
        let result = SpineScan::run_with_marrow(temp.path(), &[], "bundle-root.typ").unwrap();

        assert_eq!(result.files.len(), 1, "only index.typ is a vertebra");
        assert!(
            result
                .files
                .iter()
                .all(|p| p.file_name().unwrap() != "bundle-root.typ"),
            "the configured marrow file must not be scanned as a vertebra"
        );
    }

    /// Marrow statements are emitted after every `#document` block, at bundle
    /// root, where `document()`/`asset()` are legal.
    #[test]
    fn bundle_source_emits_marrow_after_documents() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("index.typ"), "= Index").unwrap();

        let scan = SpineScan::run(&content, &[]).unwrap();
        let spine = VirtualSpine::build(
            scan,
            root,
            SpineLayout::OnePerVertebra {
                ext: "html".into(),
                format: "html".into(),
            },
        )
        .unwrap()
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

    #[test]
    fn scan_numeric_prefix_dir_title() {
        let temp = create_test_dir_with_files(&["01-basics/setup.typ"]);
        let result = SpineScan::run(temp.path(), &[]).unwrap();

        let basics = find_node(&result.tree, "01-basics");
        assert_eq!(basics.title, Some("Basics".to_string()));
    }

    #[test]
    fn scan_exclude_prunes_subtree() {
        let temp = create_test_dir_with_files(&["drafts/wip.typ", "keep.typ"]);
        let result = SpineScan::run(temp.path(), &["drafts/**".to_string()]).unwrap();

        assert!(result.tree.iter().all(|n| n.segment != "drafts"));
        assert!(result.tree.iter().any(|n| n.segment == "keep"));
        assert_eq!(result.files.len(), 1);
    }

    #[test]
    fn scan_empty_after_exclude_errors() {
        let temp = create_test_dir_with_files(&["only.typ"]);
        let result = SpineScan::run(temp.path(), &["only.typ".to_string()]);
        match result {
            Err(e) => assert!(e.to_string().contains("need at least one .typ file")),
            Ok(_) => panic!("expected empty-scan error"),
        }
    }

    // ── apply_sections tests ─────────────────────────────────────────────────

    fn section(name: &str, include: &[&str]) -> SpineSection {
        SpineSection {
            name: name.into(),
            title: None,
            include: include.iter().map(|s| s.to_string()).collect(),
            section: Vec::new(),
        }
    }

    #[test]
    fn apply_sections_groups_flat_files() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ", "c.typ"]);
        let scan = SpineScan::run(temp.path(), &[]).unwrap();
        let out = scan
            .apply_sections(temp.path(), &[section("guide", &["a.typ", "b.typ"])])
            .unwrap();

        // c stays top-level; guide is a group node holding a and b.
        assert_eq!(out.files.len(), 3);
        let guide = out.tree.iter().find(|n| n.segment == "guide").unwrap();
        assert!(guide.vertebra.is_none()); // non-clickable group
        assert_eq!(guide.title.as_deref(), Some("Guide")); // derived from name
        let child_segs: Vec<&str> = guide.children.iter().map(|c| c.segment.as_str()).collect();
        assert_eq!(child_segs, vec!["a", "b"]);
        assert!(
            out.tree
                .iter()
                .any(|n| n.segment == "c" && n.vertebra.is_some())
        );
        // Children reindexed to valid file positions.
        for c in &guide.children {
            let idx = c.vertebra.expect("section child is a leaf vertebra");
            assert!(idx < out.files.len());
        }
    }

    #[test]
    fn apply_sections_nests_subsections() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ", "c.typ"]);
        let scan = SpineScan::run(temp.path(), &[]).unwrap();
        let mut guide = section("guide", &["a.typ"]);
        guide.section = vec![section("advanced", &["c.typ"])];
        let out = scan.apply_sections(temp.path(), &[guide]).unwrap();

        let guide = out.tree.iter().find(|n| n.segment == "guide").unwrap();
        // guide holds leaf a, then nested group advanced holding c.
        assert_eq!(guide.children[0].segment, "a");
        let advanced = guide
            .children
            .iter()
            .find(|n| n.segment == "advanced")
            .unwrap();
        assert!(advanced.vertebra.is_none());
        assert_eq!(advanced.children[0].segment, "c");
        assert!(out.tree.iter().any(|n| n.segment == "b"));
    }

    #[test]
    fn apply_sections_title_derived_strips_numeric_prefix() {
        let temp = create_test_dir_with_files(&["a.typ"]);
        let scan = SpineScan::run(temp.path(), &[]).unwrap();
        let out = scan
            .apply_sections(temp.path(), &[section("01-guide", &["a.typ"])])
            .unwrap();
        let guide = out.tree.iter().find(|n| n.segment == "01-guide").unwrap();
        assert_eq!(guide.title.as_deref(), Some("Guide")); // prefix stripped for title, kept in segment
    }

    #[test]
    fn apply_sections_include_no_match_errors() {
        let temp = create_test_dir_with_files(&["a.typ"]);
        let scan = SpineScan::run(temp.path(), &[]).unwrap();
        let err = scan
            .apply_sections(temp.path(), &[section("guide", &["nope.typ"])])
            .unwrap_err();
        assert!(err.to_string().contains("matched no files"));
    }

    #[test]
    fn apply_sections_duplicate_sibling_name_errors() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ"]);
        let scan = SpineScan::run(temp.path(), &[]).unwrap();
        let err = scan
            .apply_sections(
                temp.path(),
                &[section("guide", &["a.typ"]), section("guide", &["b.typ"])],
            )
            .unwrap_err();
        assert!(err.to_string().contains("duplicate spine section"));
    }

    #[test]
    fn apply_sections_empty_is_noop() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ"]);
        let scan = SpineScan::run(temp.path(), &[]).unwrap();
        let before = scan.files.len();
        let out = scan.apply_sections(temp.path(), &[]).unwrap();
        assert_eq!(out.files.len(), before);
    }
}

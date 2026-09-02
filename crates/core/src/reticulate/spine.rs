use crate::config::SpineSection;
use crate::parser;
use crate::reticulate::bundle_source::BundleSource;
use crate::reticulate::document_meta::DocumentTitle;
use crate::reticulate::handle::Handle;
use crate::synth::typst_literal::TypstLiteral;
use crate::synth::typst_source::TypstStmt;
use crate::util::path::to_forward_slash;
use crate::{MARROW_FILE, RESERVED_META_LABEL_PREFIX, Result, RheoError, TYP_EXT};
use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::Hash;
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

/// Compile one glob pattern with `literal_separator` (so `*` doesn't cross
/// `/` while `**` still descends), wrapping a compile failure as a
/// project-config error naming the pattern and `context` (a noun phrase
/// ending in "glob", e.g. "exclude glob", "spine include glob").
fn compile_glob(pattern: &str, context: &str) -> Result<Glob> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| RheoError::project_config(format!("invalid {context} '{pattern}': {e}")))
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

    /// Compile exclude globs into a path-aware [`GlobSet`] (matched against
    /// content_dir-relative, forward-slashed paths). `literal_separator` keeps
    /// `*` from crossing `/` while `**` still descends, matching the documented
    /// exclude semantics.
    fn build_exclude_set(exclude: &[String]) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();
        for g in exclude {
            builder.add(compile_glob(g, "exclude glob")?);
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

    /// Return `true` if `path` (relative to `content_dir`) matches any exclude glob.
    fn is_excluded(content_dir: &Path, path: &Path, exclude: &GlobSet) -> bool {
        let rel = path.strip_prefix(content_dir).unwrap_or(path);
        exclude.is_match(to_forward_slash(rel))
    }

    /// Read a directory's entries sorted by filename, for deterministic scan order.
    fn read_sorted_entries(dir: &Path) -> Result<Vec<fs::DirEntry>> {
        let mut entries: Vec<fs::DirEntry> = fs::read_dir(dir)
            .map_err(|e| {
                RheoError::project_config(format!("failed to read dir '{}': {}", dir.display(), e))
            })?
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        Ok(entries)
    }

    /// Returns `true` if `path` has the `.typ` extension.
    fn is_typ_file(path: &Path) -> bool {
        path.extension().and_then(|e| e.to_str()) == Some(&TYP_EXT[1..])
    }

    /// Register `path` as the next vertebra in `files` and build its leaf
    /// [`SpineNode`] (segment derived from the file stem, no children).
    fn push_typ_leaf(path: &Path, files: &mut Vec<PathBuf>) -> SpineNode {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let idx = files.len();
        files.push(path.to_path_buf());
        SpineNode::leaf(Handle::sanitize_segment(stem), idx)
    }

    /// Scan one directory, recursing into subdirectories. Returns the child
    /// node list for `dir`; pushes discovered files into `files` in pre-order.
    fn scan_dir(
        content_dir: &Path,
        dir: &Path,
        exclude: &GlobSet,
        files: &mut Vec<PathBuf>,
    ) -> Result<Vec<SpineNode>> {
        let mut nodes = Vec::new();

        for entry in Self::read_sorted_entries(dir)? {
            let path = entry.path();

            if Self::is_excluded(content_dir, &path, exclude) {
                continue;
            }

            if path.is_dir() {
                if let Some(node) = Self::scan_subdir(content_dir, &path, exclude, files)? {
                    nodes.push(node);
                }
            } else if Self::is_typ_file(&path) {
                // Root-level index.typ is a normal leaf; only nested dirs treat
                // it as a landing page (handled in scan_subdir).
                nodes.push(Self::push_typ_leaf(&path, files));
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

        // Excluded once, up front — both the landing-file search and the
        // children loop below read this same pre-filtered list.
        let entries: Vec<PathBuf> = Self::read_sorted_entries(dir)?
            .into_iter()
            .map(|e| e.path())
            .filter(|p| !Self::is_excluded(content_dir, p, exclude))
            .collect();

        // Find the landing file: prefer index.typ, else <dirname>.typ.
        let index_name = format!("index{}", TYP_EXT);
        let named_name = format!("{}{}", dirname, TYP_EXT);
        let named = |name: &str| {
            entries
                .iter()
                .find(|p| p.file_name().and_then(|n| n.to_str()) == Some(name))
                .cloned()
        };
        let landing_path = named(&index_name).or_else(|| named(&named_name));

        let landing_idx = landing_path.as_ref().map(|landing| {
            let idx = files.len();
            files.push(landing.clone());
            idx
        });

        // Recurse for children, skipping the landing file itself.
        let mut children = Vec::new();
        for path in &entries {
            if Some(path) == landing_path.as_ref() {
                continue;
            }

            if path.is_dir() {
                if let Some(node) = Self::scan_subdir(content_dir, path, exclude, files)? {
                    children.push(node);
                }
            } else if Self::is_typ_file(path) {
                children.push(Self::push_typ_leaf(path, files));
            }
        }

        let segment = Handle::sanitize_segment(&dirname);
        Ok(match landing_idx {
            Some(idx) => Some(SpineNode::landing(segment, idx, children)),
            // Empty subtree after exclusion/pruning: drop the whole node.
            None if children.is_empty() => None,
            None => Some(SpineNode::group(
                segment,
                Self::prettify(&dirname),
                children,
            )),
        })
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
        let leaves = Self::collect_leaf_files(&roots);

        // Build virtual section nodes, claiming leaf files (each to the first
        // section, pre-order, whose include matches it).
        let mut claimed: HashSet<PathBuf> = HashSet::new();
        let virtual_nodes =
            Self::build_section_nodes(content_dir, sections, &leaves, &mut claimed)?;

        // Remove claimed leaves from the scanned tree, pruning emptied groups.
        PathNode::retain_unclaimed(&mut roots, &claimed);

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
            if Handle::sanitize_segment(&s.name).is_empty() {
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

    /// Convert an indexed [`SpineNode`] tree into a path-carrying working
    /// tree: a structural [`Node::map`] from vertebra index to real path.
    fn to_path_nodes(nodes: &[SpineNode], files: &[PathBuf]) -> Vec<PathNode> {
        nodes
            .iter()
            .map(|n| n.map(&mut |&i| files[i].clone()))
            .collect()
    }

    /// Collect every movable leaf file path (a childless landing node) —
    /// candidates a `[[spine.section]]` include list may claim.
    fn collect_leaf_files(nodes: &[PathNode]) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for n in nodes {
            n.visit("", &mut |_, node| {
                if node.is_leaf()
                    && let NodeKind::Landing(p) = &node.kind
                {
                    out.push(p.clone());
                }
            });
        }
        out
    }

    /// Build virtual-directory nodes from `sections`, claiming leaf files.
    fn build_section_nodes(
        content_dir: &Path,
        sections: &[SpineSection],
        leaves: &[PathBuf],
        claimed: &mut HashSet<PathBuf>,
    ) -> Result<Vec<PathNode>> {
        let matcher = OrderedGlobMatch::new(content_dir, leaves);
        let mut result = Vec::new();
        for s in sections {
            // A file is claimed by the first section that matches; the whole
            // section errors only if every one of its patterns matched nothing.
            let matched = matcher.resolve(
                &s.include,
                claimed,
                &format!("spine section '{}' include glob", s.name),
            )?;
            if !s.include.is_empty() && matched.is_empty() {
                return Err(RheoError::project_config(format!(
                    "spine section '{}' include matched no files",
                    s.name
                )));
            }

            let mut children: Vec<PathNode> = matched
                .into_iter()
                .map(|p| {
                    let segment = Handle::sanitize_segment(
                        p.file_stem().and_then(|s| s.to_str()).unwrap_or_default(),
                    );
                    PathNode::leaf(segment, p)
                })
                .collect();
            children.extend(Self::build_section_nodes(
                content_dir,
                &s.section,
                leaves,
                claimed,
            )?);

            result.push(PathNode::group(
                Handle::sanitize_segment(&s.name),
                s.title.clone().unwrap_or_else(|| Self::prettify(&s.name)),
                children,
            ));
        }
        Ok(result)
    }

    /// Apply flat `[spine] include` (knob 3): replace the scan order with an
    /// explicit ordered glob list, dropping any leaf it does not match. Unlike
    /// `apply_sections`, a matched leaf keeps its scanned `PathNode` untouched —
    /// no group wrapper — so its handle and output path never change, only its
    /// position. Returns `self` unchanged when `include` is empty.
    pub fn apply_include(self, content_dir: &Path, include: &[String]) -> Result<SpineScan> {
        if include.is_empty() {
            return Ok(self);
        }

        let roots = Self::to_path_nodes(&self.tree, &self.files);
        let leaf_nodes = Self::collect_leaf_nodes(&roots);
        let mut by_path: HashMap<PathBuf, PathNode> = leaf_nodes
            .into_iter()
            .filter_map(|n| n.vertebra().cloned().map(|f| (f, n)))
            .collect();
        let leaves: Vec<PathBuf> = by_path.keys().cloned().collect();

        let matcher = OrderedGlobMatch::new(content_dir, &leaves);
        let mut claimed = HashSet::new();
        let mut new_roots = Vec::new();
        for g in include {
            let pattern = std::slice::from_ref(g);
            let matched = matcher.resolve(pattern, &mut claimed, "spine include glob")?;
            if matched.is_empty() {
                return Err(RheoError::project_config(format!(
                    "spine include pattern '{}' matched no files",
                    g
                )));
            }
            new_roots.extend(matched.into_iter().filter_map(|p| by_path.remove(&p)));
        }

        let mut files = Vec::new();
        let tree = Self::reindex(&new_roots, &mut files);
        if files.is_empty() {
            return Err(RheoError::project_config(
                "spine is empty after applying include",
            ));
        }
        Ok(SpineScan { files, tree })
    }

    /// Extract every genuine leaf (childless landing) node, discarding
    /// non-movable group/landing-page structure around it — the counterpart
    /// of [`Self::collect_leaf_files`] that yields owned nodes (segment
    /// intact) rather than just their paths.
    fn collect_leaf_nodes(nodes: &[PathNode]) -> Vec<PathNode> {
        let mut out = Vec::new();
        for n in nodes {
            n.visit("", &mut |_, node| {
                if node.is_leaf() {
                    out.push(node.clone());
                }
            });
        }
        out
    }

    /// Re-index a working tree into an indexed [`SpineNode`] tree, rebuilding the
    /// flat file list in pre-order (parent before children): a structural
    /// [`Node::map`] whose leaf transform assigns each landing file the next
    /// index by push order.
    fn reindex(nodes: &[PathNode], files: &mut Vec<PathBuf>) -> Vec<SpineNode> {
        nodes
            .iter()
            .map(|n| {
                n.map(&mut |f| {
                    let idx = files.len();
                    files.push(f.clone());
                    idx
                })
            })
            .collect()
    }
}

/// A working spine node carrying file PATHS (not indices), used while
/// transforming the scanned tree before re-indexing into [`SpineNode`].
type PathNode = Node<PathBuf>;

/// Ordered-glob resolution shared by `[[spine.section]] include` and flat
/// `[spine] include`: matches patterns against unclaimed leaves in listed
/// order (ties within one pattern broken lexicographically), claiming every
/// match so neither a later pattern nor another caller can reuse it. Callers
/// decide what an empty result means — a section only cares whether its whole
/// include list matched nothing, flat include whether one pattern did.
struct OrderedGlobMatch<'a> {
    content_dir: &'a Path,
    leaves: &'a [PathBuf],
}

impl<'a> OrderedGlobMatch<'a> {
    fn new(content_dir: &'a Path, leaves: &'a [PathBuf]) -> Self {
        Self {
            content_dir,
            leaves,
        }
    }

    /// Resolve `patterns` in listed order, claiming matches as they're found.
    /// `context` is passed straight through to [`compile_glob`] (a noun phrase
    /// ending in "glob") for the invalid-glob error.
    fn resolve(
        &self,
        patterns: &[String],
        claimed: &mut HashSet<PathBuf>,
        context: &str,
    ) -> Result<Vec<PathBuf>> {
        let mut matched = Vec::new();
        for g in patterns {
            let matcher = compile_glob(g, context)?.compile_matcher();
            let mut ms: Vec<PathBuf> = self
                .leaves
                .iter()
                .filter(|p| !claimed.contains(*p))
                .filter(|p| {
                    let rel = p.strip_prefix(self.content_dir).unwrap_or(p);
                    matcher.is_match(to_forward_slash(rel))
                })
                .cloned()
                .collect();
            ms.sort();
            for m in &ms {
                claimed.insert(m.clone());
            }
            matched.extend(ms);
        }
        Ok(matched)
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

/// One node in a spine tree, generic over the payload of a "landing" node —
/// one that resolves to a file. [`SpineNode`] (`L = usize`, a vertebra index)
/// is the final indexed tree; [`PathNode`] (`L = PathBuf`) is the working
/// tree `apply_sections`/`apply_include` reshape before re-indexing.
///
/// A node is exactly one of two shapes ([`NodeKind`]) — never neither, never
/// a pair of `Option`s that could disagree:
/// - [`NodeKind::Landing`]: this node itself resolves to a file. `children`
///   may still be empty (an ordinary leaf) or non-empty (a directory whose
///   `index.typ`/`<dirname>.typ` landing file gives the directory itself a
///   handle, alongside its own children) — landing-ness and having children
///   are independent, so it lives on `Node` rather than inside the enum.
/// - [`NodeKind::Group`]: no landing file; a non-clickable directory/section
///   with its own display title, nesting its (always non-empty) children.
#[derive(Debug, Clone)]
pub struct Node<L> {
    /// Handle segment contributed by this node (dir name, file stem, or
    /// section name). For the trivial flat tree this is the vertebra's full
    /// handle.
    pub segment: String,
    pub kind: NodeKind<L>,
    /// Child nodes, in order.
    pub children: Vec<Node<L>>,
}

/// See [`Node`] for what each variant means and why `children` lives outside it.
#[derive(Debug, Clone)]
pub enum NodeKind<L> {
    Landing(L),
    Group(String),
}

/// The final spine tree: `L = usize` indexes into `VirtualSpine.vertebrae`.
pub type SpineNode = Node<usize>;

impl<L> Node<L> {
    fn leaf(segment: String, payload: L) -> Self {
        Node {
            segment,
            kind: NodeKind::Landing(payload),
            children: Vec::new(),
        }
    }

    fn landing(segment: String, payload: L, children: Vec<Node<L>>) -> Self {
        Node {
            segment,
            kind: NodeKind::Landing(payload),
            children,
        }
    }

    fn group(segment: String, title: String, children: Vec<Node<L>>) -> Self {
        Node {
            segment,
            kind: NodeKind::Group(title),
            children,
        }
    }

    /// This node's own landing payload, if it resolves to a file (leaf or
    /// landing directory). `None` for a pure group node.
    pub fn vertebra(&self) -> Option<&L> {
        match &self.kind {
            NodeKind::Landing(p) => Some(p),
            NodeKind::Group(_) => None,
        }
    }

    /// This node's own display title. Only a group node carries one — a
    /// landing node's display title comes from the vertebra it points at.
    pub fn title(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Group(t) => Some(t.as_str()),
            NodeKind::Landing(_) => None,
        }
    }

    /// True for a genuine leaf: a landing node with no children (as opposed
    /// to a landing directory, which also has a payload but nests children).
    fn is_leaf(&self) -> bool {
        self.children.is_empty() && matches!(self.kind, NodeKind::Landing(_))
    }

    /// Pre-order structural transform: rebuild this node with its landing
    /// payload passed through `f`, shape otherwise preserved. `f` runs on
    /// this node before its children, so a stateful `f` (e.g. one assigning
    /// fresh indices by push order) numbers a node before its descendants.
    fn map<M>(&self, f: &mut impl FnMut(&L) -> M) -> Node<M> {
        let kind = match &self.kind {
            NodeKind::Landing(p) => NodeKind::Landing(f(p)),
            NodeKind::Group(t) => NodeKind::Group(t.clone()),
        };
        Node {
            segment: self.segment.clone(),
            kind,
            children: self.children.iter().map(|c| c.map(f)).collect(),
        }
    }

    /// Post-order (bottom-up) fold: build a `T` for every child first, then
    /// combine this node with its children's `T`s via `f`.
    fn fold<T>(&self, f: &mut impl FnMut(&Node<L>, Vec<T>) -> T) -> T {
        let children = self.children.iter().map(|c| c.fold(f)).collect();
        f(self, children)
    }

    /// Pre-order walk, threading the `:`-joined handle-path from the root
    /// down to (and including) each node's own segment. `f` receives that
    /// path and the node.
    fn visit(&self, prefix: &str, f: &mut impl FnMut(&str, &Node<L>)) {
        let path = if prefix.is_empty() {
            self.segment.clone()
        } else {
            format!("{prefix}:{}", self.segment)
        };
        f(&path, self);
        for c in &self.children {
            c.visit(&path, f);
        }
    }
}

impl<L: Eq + Hash> Node<L> {
    /// Remove claimed leaf payloads from the tree in place, dropping any
    /// group node left with no children.
    fn retain_unclaimed(nodes: &mut Vec<Node<L>>, claimed: &HashSet<L>) {
        nodes.retain_mut(|n| {
            Self::retain_unclaimed(&mut n.children, claimed);
            match &n.kind {
                NodeKind::Landing(p) => !(n.children.is_empty() && claimed.contains(p)),
                NodeKind::Group(_) => !n.children.is_empty(),
            }
        });
    }
}

/// Every vertebra index the tree references, in pre-order: a node's own
/// landing index (if any), then its children's, regardless of whether this
/// node itself yielded one.
fn tree_indices(tree: &[SpineNode]) -> Vec<usize> {
    let mut indices = Vec::new();
    for node in tree {
        node.visit("", &mut |_, n| {
            if let NodeKind::Landing(i) = &n.kind {
                indices.push(*i);
            }
        });
    }
    indices
}

/// The per-format values that ride on `sys.inputs.rheo-context` alongside the
/// spine itself. See [`VirtualSpine::global_context`].
pub struct FormatContext<'a> {
    /// The rheo output-format name (`"html"`/`"epub"`). `None` for PDF, which
    /// sets no rheo target and falls back to Typst's native `target()`.
    pub target: Option<&'a str>,
    /// The output file extension (`"html"`/`"xhtml"`) — what `typ/rheo.typ`
    /// reads to build cross-vertebra link hrefs. Gated exactly like `target`.
    pub ext: Option<&'a str>,
    /// The resolved per-format `reset-footnotes` toggle. Unlike `target`/`ext`
    /// this is always present; `typ/rheo.typ` ANDs it with the per-page `ext`
    /// gate, so it only ever takes effect for HTML/EPUB.
    pub reset_footnotes: bool,
    /// Rust's own post-compile `DocumentInfo` title for a vertebra whose
    /// beacon read it wrong — a title set inside a bounded code block, see
    /// `docs/limitations.md`. Empty on the ordinary single pass; populated only
    /// by the gated second pass of `Build::compile_bundle_once`.
    pub title_overrides: &'a HashMap<String, String>,
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

    /// Fill `handles[i]` with the `:`-joined segment path from the tree root to
    /// the node whose `vertebra()` is `Some(i)`, walking pre-order. Group nodes
    /// contribute their segment as a prefix to descendants without claiming a slot.
    fn assign_handles(nodes: &[SpineNode], handles: &mut [Handle]) {
        for n in nodes {
            n.visit("", &mut |path, node| {
                if let Some(&i) = node.vertebra()
                    && let Some(slot) = handles.get_mut(i)
                {
                    *slot = Handle::new(path.to_string());
                }
            });
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
        let mut handles: Vec<Handle> = vec![Handle::default(); files.len()];
        Self::assign_handles(&tree, &mut handles);

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
    /// `sys.inputs` is global to the whole bundle compile, so it carries only
    /// the parts identical across vertebrae: `spine`/`spine-flat`, the
    /// compiling rheo's own `rheo-version` (a package reads it to enforce a
    /// minimum rheo, treating its absence as "older than the release that
    /// added it"), and the per-format values in `format` — see
    /// [`FormatContext`] for each. Packages read `sys.inputs.rheo-context` to
    /// detect a rheo build without referencing the per-file `rheo-context()`,
    /// which additionally carries that file's `handle`.
    ///
    /// `title-overrides` is serialized as an array of `(handle, title)` dicts
    /// rather than a dict keyed by handle, since a handle like
    /// `"chapters:intro"` is not a valid Typst identifier — the same reason
    /// `spine-flat` is an array of handle-bearing dicts.
    pub fn global_context(&self, format: FormatContext<'_>) -> TypstLiteral {
        let FormatContext {
            target,
            ext,
            reset_footnotes,
            title_overrides,
        } = format;
        let mut fields = vec![
            ("spine".to_string(), self.spine_tree()),
            ("spine-flat".to_string(), self.spine_flat()),
            (
                "rheo-version".to_string(),
                TypstLiteral::str(env!("CARGO_PKG_VERSION")),
            ),
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
        fields.push((
            "title-overrides".to_string(),
            TypstLiteral::Array(
                title_overrides
                    .iter()
                    .map(|(handle, title)| {
                        TypstLiteral::Dict(vec![
                            ("handle".to_string(), TypstLiteral::str(handle.as_str())),
                            ("title".to_string(), TypstLiteral::str(title.as_str())),
                        ])
                    })
                    .collect(),
            ),
        ));
        TypstLiteral::Dict(fields)
    }

    /// The structured spine tree as a [`TypstLiteral`] array of recursive node
    /// dicts. See [`Self::node_literal`] for the node key set.
    fn spine_tree(&self) -> TypstLiteral {
        TypstLiteral::Array(self.tree.iter().map(|n| self.node_literal(n)).collect())
    }

    /// Serialize one [`SpineNode`] (and its descendants) to its `spine` dict
    /// shape: `title`/`handle`/`path`/`children`. Per-vertebra document
    /// metadata is not part of this shape — read it live via
    /// `rheo-context().metadata-of` (see [`TypstStmt::MetadataHelper`]).
    fn node_literal(&self, node: &SpineNode) -> TypstLiteral {
        node.fold(&mut |n, children| {
            let (handle, path, title) = match n.vertebra().and_then(|&i| self.vertebrae.get(i)) {
                Some(v) => (
                    TypstLiteral::str(v.handle.as_str()),
                    TypstLiteral::str(v.rel_path.as_str()),
                    TypstLiteral::str(v.title.as_str()),
                ),
                None => (
                    TypstLiteral::None,
                    TypstLiteral::None,
                    TypstLiteral::str(n.title().unwrap_or(n.segment.as_str())),
                ),
            };
            TypstLiteral::Dict(vec![
                ("title".to_string(), title),
                ("handle".to_string(), handle),
                ("path".to_string(), path),
                ("children".to_string(), TypstLiteral::Array(children)),
            ])
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A per-page (HTML) format context with footnote reset on.
    fn html_context(title_overrides: &HashMap<String, String>) -> FormatContext<'_> {
        FormatContext {
            target: Some("html"),
            ext: Some("html"),
            reset_footnotes: true,
            title_overrides,
        }
    }

    /// A combined-PDF format context: no target, no ext.
    fn pdf_context(title_overrides: &HashMap<String, String>) -> FormatContext<'_> {
        FormatContext {
            target: None,
            ext: None,
            reset_footnotes: true,
            title_overrides,
        }
    }

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
        // Neither the spine tree nor spine-flat entries carry a `metadata`
        // key — `Vertebra.title` is purely path-derived, so this vertebra's
        // spine-entry title is "Post" (from the filename), not the
        // `#set document(title: ...)` value. Read live document metadata via
        // `rheo-context().metadata-of` instead.
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
            .global_context(html_context(&HashMap::new()))
            .serialize();
        assert!(global_html.contains("target: \"html\""));
        assert!(global_html.contains("ext: \"html\""));
        // The resolved reset-footnotes toggle is always present (unlike target/ext).
        assert!(global_html.contains("reset-footnotes: true"));

        // Epub keeps `target` "epub" but `ext` "xhtml"; a false toggle is threaded through.
        let global_epub = spine
            .global_context(FormatContext {
                target: Some("epub"),
                ext: Some("xhtml"),
                reset_footnotes: false,
                title_overrides: &HashMap::new(),
            })
            .serialize();
        assert!(global_epub.contains("target: \"epub\""));
        assert!(global_epub.contains("ext: \"xhtml\""));
        assert!(global_epub.contains("reset-footnotes: false"));

        // None (PDF) -> no `target` or `ext` field, but reset-footnotes is still present.
        let global_without = spine
            .global_context(pdf_context(&HashMap::new()))
            .serialize();
        assert!(!global_without.contains("target:"));
        assert!(!global_without.contains("ext:"));
        assert!(global_without.contains("reset-footnotes: true"));
    }

    #[test]
    fn global_context_carries_rheo_version() {
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

        let expected = format!("rheo-version: \"{}\"", env!("CARGO_PKG_VERSION"));
        assert!(
            spine
                .global_context(pdf_context(&HashMap::new()))
                .serialize()
                .contains(&expected)
        );
    }

    #[test]
    fn global_context_title_overrides_serializes_as_handle_title_array() {
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

        // Empty by default (ordinary single pass): still a present, empty array.
        let empty = spine
            .global_context(pdf_context(&HashMap::new()))
            .serialize();
        assert!(empty.contains("title-overrides: ()"));

        // A handle like "chapters:intro" is not a valid Typst identifier, so
        // overrides must be an array of dicts, not a dict keyed by handle.
        let overrides = HashMap::from([("chapters:intro".to_string(), "Real Title".to_string())]);
        let with_override = spine.global_context(pdf_context(&overrides)).serialize();
        assert!(
            with_override.contains(
                "title-overrides: ((handle: \"chapters:intro\", title: \"Real Title\"),)"
            )
        );
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
        assert!(guide.vertebra().is_some());
        assert_eq!(guide.segment, "guide");

        let a = find_node(&guide.children, "a");
        assert!(a.vertebra().is_some());
        let _b = find_node(&guide.children, "b");

        let deep = find_node(&guide.children, "deep");
        assert!(deep.vertebra().is_none());
        let x = find_node(&deep.children, "x");
        assert!(x.vertebra().is_some());
    }

    #[test]
    fn scan_dir_without_index_is_group_node() {
        let temp = create_test_dir_with_files(&["extras/note.typ"]);
        let result = SpineScan::run(temp.path(), &[]).unwrap();

        let extras = find_node(&result.tree, "extras");
        assert!(extras.vertebra().is_none());
        assert_eq!(extras.title(), Some("Extras"));
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

    #[test]
    fn scan_numeric_prefix_dir_title() {
        let temp = create_test_dir_with_files(&["01-basics/setup.typ"]);
        let result = SpineScan::run(temp.path(), &[]).unwrap();

        let basics = find_node(&result.tree, "01-basics");
        assert_eq!(basics.title(), Some("Basics"));
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
        assert!(guide.vertebra().is_none()); // non-clickable group
        assert_eq!(guide.title(), Some("Guide")); // derived from name
        let child_segs: Vec<&str> = guide.children.iter().map(|c| c.segment.as_str()).collect();
        assert_eq!(child_segs, vec!["a", "b"]);
        assert!(
            out.tree
                .iter()
                .any(|n| n.segment == "c" && n.vertebra().is_some())
        );
        // Children reindexed to valid file positions.
        for c in &guide.children {
            let idx = *c.vertebra().expect("section child is a leaf vertebra");
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
        assert!(advanced.vertebra().is_none());
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
        assert_eq!(guide.title(), Some("Guide")); // prefix stripped for title, kept in segment
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

    // ── apply_include tests ─────────────────────────────────────────────────

    #[test]
    fn apply_include_reorders_flat_and_drops_unmatched() {
        let temp = create_test_dir_with_files(&["b.typ", "a.typ", "c.typ", "d.typ"]);
        let scan = SpineScan::run(temp.path(), &[]).unwrap();
        let out = scan
            .apply_include(
                temp.path(),
                &["b.typ".into(), "a.typ".into(), "c.typ".into()],
            )
            .unwrap();

        let stems: Vec<&str> = out
            .files
            .iter()
            .map(|f| f.file_stem().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(stems, vec!["b", "a", "c"]); // reordered, d dropped entirely

        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(out, temp.path(), layout).unwrap();
        let paths: Vec<&str> = spine
            .vertebrae
            .iter()
            .map(|v| v.output_path.as_str())
            .collect();
        // Flat output paths — no group segment, unlike apply_sections
        // (contrast nested_handle_maps_to_slash_output_path).
        assert_eq!(paths, vec!["b.html", "a.html", "c.html"]);
    }

    #[test]
    fn apply_include_pattern_no_match_errors() {
        let temp = create_test_dir_with_files(&["a.typ"]);
        let scan = SpineScan::run(temp.path(), &[]).unwrap();
        let err = scan
            .apply_include(temp.path(), &["nope.typ".to_string()])
            .unwrap_err();
        assert!(err.to_string().contains("matched no files"));
    }

    #[test]
    fn apply_include_empty_is_noop() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ"]);
        let scan = SpineScan::run(temp.path(), &[]).unwrap();
        let before = scan.files.len();
        let out = scan.apply_include(temp.path(), &[]).unwrap();
        assert_eq!(out.files.len(), before);
    }
}

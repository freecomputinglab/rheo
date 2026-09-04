use super::SpineScan;
use super::tree::SpineNode;
use crate::reticulate::handle::Handle;
use crate::util::path::to_forward_slash;
use crate::{Result, RheoError, TYP_EXT};
use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::{Path, PathBuf};

/// Compile one glob pattern with `literal_separator` (so `*` doesn't cross
/// `/` while `**` still descends), wrapping a compile failure as a
/// project-config error naming the pattern and `context` (a noun phrase
/// ending in "glob", e.g. "exclude glob", "spine include glob").
pub(super) fn compile_glob(pattern: &str, context: &str) -> Result<Glob> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| RheoError::project_config(format!("invalid {context} '{pattern}': {e}")))
}

impl SpineScan {
    /// Compile exclude globs into a path-aware [`GlobSet`] (matched against
    /// content_dir-relative, forward-slashed paths). `literal_separator` keeps
    /// `*` from crossing `/` while `**` still descends, matching the documented
    /// exclude semantics.
    pub(super) fn build_exclude_set(exclude: &[String]) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();
        for g in exclude {
            builder.add(compile_glob(g, "exclude glob")?);
        }
        builder
            .build()
            .map_err(|e| RheoError::project_config(format!("invalid exclude globs: {}", e)))
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

    /// Every `.typ` file under `dir`, recursively, in the same sorted pre-order
    /// the spine scan walks. A missing directory yields nothing.
    ///
    /// Deliberately unfiltered — no `exclude`, no marrow skip, no sections: it
    /// answers "what Typst does this project contain" (which packages it
    /// imports, where its content root is), not "what belongs to the spine".
    /// One walker for both questions, so they cannot drift apart.
    pub fn typ_files(dir: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        if dir.is_dir() {
            Self::collect_typ_files(dir, &mut files)?;
        }
        Ok(files)
    }

    fn collect_typ_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        for entry in Self::read_sorted_entries(dir)? {
            let path = entry.path();
            if path.is_dir() {
                Self::collect_typ_files(&path, files)?;
            } else if Self::is_typ_file(&path) {
                files.push(path);
            }
        }
        Ok(())
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
    pub(super) fn scan_dir(
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
    pub(super) fn prettify(dirname: &str) -> String {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MARROW_FILE;
    use crate::reticulate::spine::{create_test_dir_with_files, find_node};

    /// The project-wide enumeration answers a different question from the spine
    /// scan: it keeps the marrow file and anything `exclude` would drop, because
    /// a package imported from one of those still contributes assets.
    #[test]
    fn typ_files_are_sorted_and_unfiltered() {
        let temp = create_test_dir_with_files(&[
            "intro.typ",
            "drafts/wip.typ",
            "guide/a.typ",
            MARROW_FILE,
            "notes.md",
        ]);
        let root = temp.path();

        let files = SpineScan::typ_files(root).unwrap();
        let rel: Vec<String> = files
            .iter()
            .map(|f| to_forward_slash(f.strip_prefix(root).unwrap()))
            .collect();
        assert_eq!(
            rel,
            vec![MARROW_FILE, "drafts/wip.typ", "guide/a.typ", "intro.typ"]
        );
    }

    #[test]
    fn typ_files_of_missing_dir_is_empty() {
        let temp = create_test_dir_with_files(&["a.typ"]);
        let files = SpineScan::typ_files(&temp.path().join("nope")).unwrap();
        assert!(files.is_empty());
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
}

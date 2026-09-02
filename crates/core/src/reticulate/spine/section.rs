use super::SpineScan;
use super::scan::compile_glob;
use super::tree::{Node, NodeKind, SpineNode};
use crate::config::SpineSection;
use crate::reticulate::handle::Handle;
use crate::util::path::to_forward_slash;
use crate::{Result, RheoError};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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

impl SpineScan {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reticulate::spine::{SpineLayout, VirtualSpine, create_test_dir_with_files};

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

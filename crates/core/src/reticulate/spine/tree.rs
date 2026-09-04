use std::collections::HashSet;
use std::hash::Hash;

/// One node in a spine tree, generic over the payload of a "landing" node —
/// one that resolves to a file. [`SpineNode`] (`L = usize`, a vertebra index)
/// is the final indexed tree; `PathNode` (`L = PathBuf`) is the working
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
    pub(super) fn leaf(segment: String, payload: L) -> Self {
        Node {
            segment,
            kind: NodeKind::Landing(payload),
            children: Vec::new(),
        }
    }

    pub(super) fn landing(segment: String, payload: L, children: Vec<Node<L>>) -> Self {
        Node {
            segment,
            kind: NodeKind::Landing(payload),
            children,
        }
    }

    pub(super) fn group(segment: String, title: String, children: Vec<Node<L>>) -> Self {
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
    pub(super) fn is_leaf(&self) -> bool {
        self.children.is_empty() && matches!(self.kind, NodeKind::Landing(_))
    }

    /// Pre-order structural transform: rebuild this node with its landing
    /// payload passed through `f`, shape otherwise preserved. `f` runs on
    /// this node before its children, so a stateful `f` (e.g. one assigning
    /// fresh indices by push order) numbers a node before its descendants.
    pub(super) fn map<M>(&self, f: &mut impl FnMut(&L) -> M) -> Node<M> {
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
    pub(super) fn fold<T>(&self, f: &mut impl FnMut(&Node<L>, Vec<T>) -> T) -> T {
        let children = self.children.iter().map(|c| c.fold(f)).collect();
        f(self, children)
    }

    /// Pre-order walk, threading the `:`-joined handle-path from the root
    /// down to (and including) each node's own segment. `f` receives that
    /// path and the node.
    pub(super) fn visit(&self, prefix: &str, f: &mut impl FnMut(&str, &Node<L>)) {
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
    pub(super) fn retain_unclaimed(nodes: &mut Vec<Node<L>>, claimed: &HashSet<L>) {
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
pub(super) fn tree_indices(tree: &[SpineNode]) -> Vec<usize> {
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

//! The `SyntaxSite` trait and the shared tree-walk engine.
//!
//! This is the core the whole `parser` module is organized around: one
//! traversal ([`walk_tree`]) drives every extractor. The individual extractors
//! live in sibling files (`labels`, `document_date`, `rheo_var`, `imports`);
//! each is a small `impl SyntaxSite` that only inspects a node.

use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// Context threaded down a syntax-tree walk, derived centrally by
/// [`descend_ctx`] as the walker descends. Visitors read the flags they care
/// about; they never compute context themselves.
#[derive(Clone, Copy)]
pub struct WalkCtx {
    /// Inside code context (function arguments, code blocks) where a `<name>`
    /// label is a *reference* (`#link(<name>)`) rather than a *definition*.
    pub in_code: bool,
    /// At the top markup level, where a `#let rheo-*` binding is file-scope.
    /// Cleared on descent into a closure, code block, or another binding's RHS.
    pub file_scope: bool,
}

impl Default for WalkCtx {
    fn default() -> Self {
        // The document root is top-level markup: file scope, not code.
        WalkCtx {
            in_code: false,
            file_scope: true,
        }
    }
}

/// Derive the context for the *children* of a node of `kind`, given the node's
/// own context. Applied once per node before recursing, so a node is inspected
/// under its parent's context while its subtree sees the updated one — e.g. a
/// top-level `#let` is itself file-scope, but its RHS is not.
///
/// This is the single place descent semantics live, so every visitor in a walk
/// agrees on what "code context" and "file scope" mean.
fn descend_ctx(kind: SyntaxKind, ctx: WalkCtx) -> WalkCtx {
    use SyntaxKind::{Args, Closure, Code, CodeBlock, LetBinding};
    WalkCtx {
        // Function args and code blocks are code context: a `<name>` inside them
        // is a reference (`#link(<name>)`), not a label definition.
        in_code: ctx.in_code || matches!(kind, Args | CodeBlock | Code),
        // A `rheo-*` binding counts only at the top markup level. Inside a
        // closure, a code block, or a binding's own RHS, file scope is left.
        file_scope: ctx.file_scope && !matches!(kind, Closure | CodeBlock | LetBinding),
    }
}

/// An element locatable across a Typst syntax tree during a single depth-first,
/// offset-tracking walk.
///
/// Implement [`visit`](SyntaxSite::visit) to inspect a node and record any
/// matches; the shared [`walk_tree`] drives traversal, handing each node its
/// byte offset and centrally-derived [`WalkCtx`]. Descent and context are the
/// walker's job — a visitor only inspects the current node.
///
/// [`MAX_SITES`](SyntaxSite::MAX_SITES) bounds how many sites
/// [`collect`](SyntaxSite::collect) gathers before the walk halts. `None`
/// collects every occurrence (e.g. every label); `Some(1)` models a
/// "find the first" single-value extractor — use [`first`](SyntaxSite::first)
/// to get that lone value as an `Option` (as `DocumentDate` does).
pub trait SyntaxSite: Sized {
    /// Stop [`collect`](SyntaxSite::collect) once this many sites are found.
    /// `None` = unbounded.
    const MAX_SITES: Option<usize> = None;

    /// Inspect `node` (whose first byte is at `offset`) under `ctx` and push any
    /// matches to `out`. `source` is provided for extractors that need line
    /// numbers or wider context; most need only the node and its offset.
    fn visit(source: &Source, node: &SyntaxNode, offset: usize, ctx: WalkCtx, out: &mut Vec<Self>);

    /// Collect sites of this one type from `source` — a single parse and a
    /// single walk, up to [`MAX_SITES`](SyntaxSite::MAX_SITES).
    fn collect(source: &Source) -> Vec<Self> {
        let root = parse_source(source);
        let mut out = Vec::new();
        walk_tree(source, &root, 0, WalkCtx::default(), &mut |s, n, o, c| {
            Self::visit(s, n, o, c, &mut out);
            Self::MAX_SITES.is_none_or(|max| out.len() < max)
        });
        out
    }

    /// The first site in document order, if any. Pairs with `MAX_SITES = Some(1)`.
    fn first(source: &Source) -> Option<Self> {
        Self::collect(source).into_iter().next()
    }
}

/// Parse `source` into a syntax tree. The one parse seam in this module, so the
/// once-only guarantee can be observed in tests.
pub(super) fn parse_source(source: &Source) -> SyntaxNode {
    #[cfg(test)]
    PARSE_COUNT.with(|c| c.set(c.get() + 1));
    typst::syntax::parse(source.text())
}

/// Depth-first walk from `node` (first byte at `offset`), invoking `visit` on
/// every node with its offset and centrally-derived context. Each node is
/// visited exactly once; returns early (yielding `false`) as soon as `visit`
/// returns `false`. This is the sole tree traversal in the module.
fn walk_tree<F>(
    source: &Source,
    node: &SyntaxNode,
    offset: usize,
    ctx: WalkCtx,
    visit: &mut F,
) -> bool
where
    F: FnMut(&Source, &SyntaxNode, usize, WalkCtx) -> bool,
{
    if !visit(source, node, offset, ctx) {
        return false;
    }
    // Children share one context, derived from this node's kind.
    let child_ctx = descend_ctx(node.kind(), ctx);
    let mut child_offset = offset;
    for child in node.children() {
        if !walk_tree(source, child, child_offset, child_ctx, visit) {
            return false;
        }
        child_offset += child.len();
    }
    true
}

/// Walk `root` once, fanning every node out to `visit`. Distinct from
/// [`SyntaxSite::collect`] in that a single traversal feeds several visitors —
/// the basis of `extract_nodes`' one-parse/one-walk guarantee.
pub(super) fn walk_once(
    source: &Source,
    root: &SyntaxNode,
    mut visit: impl FnMut(&Source, &SyntaxNode, usize, WalkCtx),
) {
    #[cfg(test)]
    WALK_COUNT.with(|c| c.set(c.get() + 1));
    walk_tree(source, root, 0, WalkCtx::default(), &mut |s, n, o, c| {
        visit(s, n, o, c);
        true
    });
}

// Instrumentation for the once-only guarantee. Thread-local so parallel test
// threads don't interfere; each guarantee test resets before measuring.
#[cfg(test)]
thread_local! {
    static PARSE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static WALK_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Reset the parse/traversal counters before a measurement.
#[cfg(test)]
pub(super) fn reset_walk_counts() {
    PARSE_COUNT.with(|c| c.set(0));
    WALK_COUNT.with(|c| c.set(0));
}

/// `(parses, traversals)` recorded since the last [`reset_walk_counts`].
#[cfg(test)]
pub(super) fn walk_counts() -> (usize, usize) {
    (PARSE_COUNT.with(|c| c.get()), WALK_COUNT.with(|c| c.get()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::Source;

    fn count_nodes(node: &SyntaxNode) -> usize {
        1 + node.children().map(count_nodes).sum::<usize>()
    }

    #[test]
    fn walk_tree_visits_each_node_once() {
        let source = Source::detached("= H <a>\n\n#let rheo-x = \"y\"\n\nSee @a.");
        let root = typst::syntax::parse(source.text());
        let expected = count_nodes(&root);

        let mut visited = 0usize;
        walk_tree(&source, &root, 0, WalkCtx::default(), &mut |_, _, _, _| {
            visited += 1;
            true
        });
        assert_eq!(visited, expected, "walk must visit each node exactly once");
    }
}

//! `rheo migrate` — best-effort, experimental migration of an older Rheo project
//! to the latest version.
//!
//! Migration is version-aware: the project's `version` field in `rheo.toml` is
//! compared against the current Rheo version, and the set of migrations that
//! span that gap is applied. See `freecomputinglab/rheo#139`.
//!
//! # Handle separator change (pre-release)
//!
//! During the pre-release development of the `#link(<handle>)` feature, the
//! path separator for nested-file handles changed from `-` to `:` (e.g.
//! `<chapters-intro>` → `<chapters:intro>`). No automatic rewrite is provided
//! for this change: `-` is also a valid label character used in ordinary
//! single-segment stems (e.g. `<title-page>`), making the two forms
//! indistinguishable without full project context. Projects that adopted the
//! old `-` scheme during that pre-release window must update their links
//! manually; `rheo migrate --dry-run` shows the new canonical handles.
//!
//! # Output format: `rheo-target` → `rheo-context.target`
//!
//! The `sys.inputs.rheo-target` key and the injected `rheo-target()` helper were
//! removed; the output format now lives on `sys.inputs.rheo-context.target`
//! and is surfaced through Typst's polyfilled `target()`. Projects upgrading from
//! an older version have their direct references rewritten (`migrate_target_references`).
//! Authors using the polyfilled `target()` need no change — it already reports
//! the output format.
//!
//! # Context binding: `rheo-context` dict → `rheo-context()` function
//!
//! The per-vertebra `rheo-context` binding, first injected as a bare data dict,
//! was encapsulated behind a zero-arg function `rheo-context()`. Projects on the
//! dict-era version have a one-line compatibility shim prepended to each file
//! that reads the binding (`migrate_context_references`), so existing
//! `rheo-context.field` / `ctx: rheo-context` code keeps working untouched.
//!
//! # Atom feed removal: `[html] feed_*` keys and `rheo-*` variables
//!
//! Atom feed generation moved to the Typst package `@rheo/feeds`: the Rust
//! generator, its `[html] feed_*` rheo.toml keys, and the generic `rheo-*`
//! `.typ` variable convention that fed it (`rheo-feed-title`,
//! `rheo-feed-updated`, `rheo-feed-exclude`) are all gone. `rheo-author` is
//! reported alongside them (same removed harvesting mechanism) even though
//! it isn't a feed key — EPUB now reads `#set document(author: ...)`
//! directly. Nothing here is rewritten: a feed's title/author/base-url/
//! inclusion rules don't map one-to-one onto the package's Typst
//! configuration, so `report_removed_feed_surface` only reports every
//! affected key and binding, with its location, pointing at `@rheo/feeds`
//! or (for `rheo-author`) the `#set document(...)` replacement.

use crate::reporter::Reporter;
use rheo_core::build::resolve_effective_content_dir;
use rheo_core::config::RETIRED_KEYS;
use rheo_core::config::manifest_version::ManifestVersion;
use rheo_core::parser::{SyntaxSite, WalkCtx};
use rheo_core::project::ProjectConfig;
use rheo_core::reticulate::SpineScan;
use rheo_core::util::path::{canonicalize_path, to_forward_slash};
use rheo_core::{Result, RheoError, Spine};
use std::collections::HashSet;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use typst_syntax::ast::AstNode;
use typst_syntax::{Source as TypstSource, SyntaxKind, SyntaxNode, ast};
use walkdir::WalkDir;

/// `#let rheo-<key>` bindings retired alongside the Rust feed generator, and
/// what replaces each.
const REMOVED_VAR_BINDINGS: &[(&str, &str)] = &[
    (
        "rheo-feed-title",
        "moved to @rheo/feeds's Typst configuration",
    ),
    (
        "rheo-feed-updated",
        "moved to @rheo/feeds's Typst configuration",
    ),
    (
        "rheo-feed-exclude",
        "moved to @rheo/feeds's Typst configuration",
    ),
    (
        "rheo-author",
        "use `#set document(author: \"...\")` instead",
    ),
];

/// One `.typ` file's source, loaded once per `migrate` run and shared across
/// every migration that reads or rewrites it — so a project's sources are
/// walked and read exactly once regardless of how many migrations touch them.
/// A rewriting migration mutates `text` in place and sets `dirty`; the report-only
/// migration only reads. `migrate_project` flushes every `dirty` source to disk
/// once, after all due migrations have run.
struct Source {
    path: PathBuf,
    text: String,
    dirty: bool,
}

/// Load every `.typ` file under `content_dir` and read it once. A file that
/// fails to read is skipped with the same `warn!` every migration used to
/// emit individually, now emitted once at load instead of per migration.
fn load_sources(content_dir: &Path) -> Vec<Source> {
    collect_typ_files(content_dir)
        .into_iter()
        .filter_map(|path| match fs::read_to_string(&path) {
            Ok(text) => Some(Source {
                path,
                text,
                dirty: false,
            }),
            Err(e) => {
                warn!(file = %path.display(), error = %e, "skipping unreadable file");
                None
            }
        })
        .collect()
}

/// One version-gated migration: `since` is the version at which its change
/// landed (projects older than that need it run), `plan` is its one-line
/// summary in the plan block, `heading` precedes its own output, and `run`
/// performs it against the sources and the `rheo.toml` document loaded once
/// by `migrate_project`. A migration always mutates its in-memory targets
/// when it finds something to change — `migrate_project` alone decides
/// afterward whether that in-memory state gets flushed to disk (dry run) or
/// not, so no migration needs to know which mode it's running in.
struct Migration {
    since: &'static str,
    plan: &'static str,
    heading: &'static str,
    run:
        fn(&ProjectConfig, &mut [Source], &mut toml_edit::DocumentMut, &mut Reporter) -> Result<()>,
}

impl Migration {
    /// Whether a project recorded at `from` needs this migration.
    /// `since` is a hardcoded semver literal covered by
    /// `migrations_since_versions_parse`.
    fn needed(&self, from: &ManifestVersion) -> bool {
        let since = ManifestVersion::parse(self.since)
            .expect("MIGRATIONS entries are covered by migrations_since_versions_parse");
        *from < since
    }
}

/// Migrations in the order they run. Each is gated on the version at which
/// its change actually landed, so a patch/minor bump only runs the migrations
/// introduced since the project's recorded version — not every historical
/// migration on every bump.
const MIGRATIONS: &[Migration] = &[
    Migration {
        since: "0.4.0",
        plan: "  - rewrite #link(\"./file.typ\") syntax to #link(<handle>)",
        heading: "\nLink rewrites:",
        run: run_link_syntax,
    },
    Migration {
        since: "0.5.0",
        plan: "  - rewrite sys.inputs.rheo-target -> sys.inputs.rheo-context.target (and rheo-target() -> target())",
        heading: "\nTarget references:",
        run: run_target_references,
    },
    Migration {
        since: "0.5.0",
        plan: "  - convert retired [spine] vertebrae glob lists to [spine] exclude",
        heading: "\nSpine config:",
        run: migrate_vertebrae_to_exclude,
    },
    Migration {
        since: "0.5.1",
        plan: "  - shim `rheo-context` binding for its new `rheo-context()` function form",
        heading: "\nContext references:",
        run: run_context_references,
    },
    Migration {
        since: "0.6.0",
        plan: "  - report removed Atom feed config keys and rheo-* variables (no rewrite)",
        heading: "\nRemoved feed surface:",
        run: run_removed_feed_surface,
    },
];

/// Run migration for the project at `path`.
///
/// `apply == false` is a dry run: it reports the version gap, prints each link
/// that would be rewritten, but writes nothing. `apply == true` rewrites links
/// and bumps the `version` field in `rheo.toml`. Every migration mutates its
/// in-memory sources/document unconditionally; `apply` is consulted in
/// exactly one place, below, which decides whether that in-memory state is
/// flushed to disk.
pub fn migrate_project(path: &Path, apply: bool, reporter: &mut Reporter) -> Result<()> {
    info!(path = %path.display(), "loading project for migration");
    let project = ProjectConfig::from_path(path, None)?;

    let config_path = project.config_path.as_ref().ok_or_else(|| {
        RheoError::project_config("no rheo.toml found for this project; nothing to migrate")
    })?;

    let from = project.config.version.clone();
    let to = ManifestVersion::current();

    info!(from = %from, to = %to, "migration target");
    reporter.line(format_args!("Project version: {from}"));
    reporter.line(format_args!("Target version:  {to}"));

    if from >= to {
        reporter.line("Project is already up to date; nothing to migrate.");
        return Ok(());
    }

    let due: Vec<&Migration> = MIGRATIONS.iter().filter(|m| m.needed(&from)).collect();

    reporter.line("\nMigrations:");
    for m in &due {
        reporter.line(m.plan);
    }
    reporter.line(format_args!("  - bump rheo.toml version to {to}"));

    // Loaded once regardless of how many due migrations read/rewrite sources
    // (empty when `due` needs none, e.g. a version bump with no pending migration).
    let mut sources = if due.is_empty() {
        Vec::new()
    } else {
        load_sources(&resolve_effective_content_dir(&project))
    };

    // One document for the whole run: migrations that touch `rheo.toml`
    // (currently just `migrate_vertebrae_to_exclude`) and the version bump
    // below all mutate this same `doc`, which is written at most once.
    let mut doc = load_toml_doc(config_path)?;

    for m in &due {
        reporter.line(m.heading);
        (m.run)(&project, &mut sources, &mut doc, reporter)?;
    }

    // Last, so the file ends up with both any migration's edits and the bump.
    bump_version(&mut doc, &to);

    if apply {
        for source in &sources {
            if source.dirty {
                fs::write(&source.path, source.text.as_bytes())
                    .map_err(|e| RheoError::io(e, format!("writing {}", source.path.display())))?;
            }
        }
        fs::write(config_path, doc.to_string())
            .map_err(|e| RheoError::io(e, format!("writing {}", config_path.display())))?;
        reporter.line(format_args!("\nBumped rheo.toml version to {to}."));
    } else {
        reporter.line("\nDry run; no changes made. Re-run with --apply to write them.");
    }
    Ok(())
}

/// Read and parse `rheo.toml` into an editable document. `toml_edit` (rather
/// than a serde round-trip) is what lets a rewrite preserve the user's
/// formatting and comments.
fn load_toml_doc(config_path: &Path) -> Result<toml_edit::DocumentMut> {
    let text = fs::read_to_string(config_path)
        .map_err(|e| RheoError::io(e, format!("reading {}", config_path.display())))?;
    text.parse().map_err(|e| {
        RheoError::project_config(format!("failed to parse {}: {}", config_path.display(), e))
    })
}

/// A `link("...")` call whose first (and only inspected) argument is a `Str`
/// ending `.typ` — a candidate for the `#link(<handle>)` rewrite.
///
/// `range` covers only the `Str` token, not the surrounding call, so splicing
/// in the handle leaves `#link(`, `)`, and any trailing content block
/// untouched. Matching on `FuncCall`/`Ident` rather than raw text also means a
/// `link(...)` mentioned inside a `//`/`/* */` comment or a raw span never
/// produces a node at all, so those false positives disappear for free.
struct LinkTarget {
    href: String,
    range: Range<usize>,
    /// The enclosing `FuncCall`'s own start offset, for the reported line
    /// number (matches the old regex, which anchored on `#link(`).
    call_start: usize,
}

impl SyntaxSite for LinkTarget {
    fn visit(
        _source: &TypstSource,
        node: &SyntaxNode,
        offset: usize,
        _ctx: WalkCtx,
        out: &mut Vec<Self>,
    ) {
        if node.kind() != SyntaxKind::FuncCall {
            return;
        }
        let Some(call) = node.cast::<ast::FuncCall>() else {
            return;
        };
        if !matches!(call.callee(), ast::Expr::Ident(id) if id.as_str() == "link") {
            return;
        }
        if let Some((href, range)) = first_str_arg(node, offset) {
            out.push(LinkTarget {
                href,
                range,
                call_start: offset,
            });
        }
    }
}

/// Rewrite old `#link("./file.typ")` syntax to the `#link(<handle>)` form.
///
/// Handles are taken from `VirtualSpine::build` (`crates/core/src/reticulate/
/// spine.rs`), which is collision-aware: the primary handle is bare (`<intro>`)
/// when the stem is unique, and path-qualified with `:` separator (`<chapters:intro>`)
/// for nested files. The `<stem.typ>` escape alias is ambiguous when stems collide,
/// so it is never used as a rewrite target.
fn migrate_link_syntax(
    project: &ProjectConfig,
    sources: &mut [Source],
    reporter: &mut Reporter,
) -> Result<()> {
    if sources.is_empty() {
        return Ok(());
    }
    let content_dir = resolve_effective_content_dir(project);

    // Canonical source path -> the handle to emit. Handle derivation is
    // format-independent, so this asks the scan for handles alone rather than
    // naming an output format it does not have.
    //
    // The primary handle is always unique: bare (`intro`) for root-level files,
    // path-qualified with ':' (`chapters:intro`) for nested files. The
    // `<stem.typ>` escape alias is basename-based and AMBIGUOUS on stem
    // collision, so it is never used as a rewrite target.
    let typ_files: Vec<PathBuf> = sources.iter().map(|s| s.path.clone()).collect();
    let handle_map = SpineScan::flat(&typ_files, &content_dir).handles_by_path();

    for source in sources.iter_mut() {
        let parent = source.path.parent().unwrap_or_else(|| Path::new(""));
        let typ_source = TypstSource::detached(source.text.as_str());

        // One walk collects every candidate; edits are spliced back-to-front below.
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();
        for link in LinkTarget::collect(&typ_source) {
            // Leave external URLs untouched.
            if link.href.contains("://") {
                continue;
            }
            let resolved = canonicalize_path(&resolve_href(&link.href, parent, &content_dir))
                .ok()
                .and_then(|c| handle_map.get(&c).cloned());
            match resolved {
                Some(target) => {
                    let line = line_number(&source.text, link.call_start);
                    reporter.rewrite(
                        &source.path,
                        line,
                        format_args!("#link(\"{}\")", link.href),
                        format_args!("#link(<{target}>)"),
                    );
                    edits.push((link.range, format!("<{target}>")));
                }
                None => {
                    warn!(file = %source.path.display(), href = %link.href, "link target is not a vertebra; skipping");
                }
            }
        }

        if !edits.is_empty() {
            apply_edits(&mut source.text, edits);
            source.dirty = true;
        }
    }

    Ok(())
}

/// Adapts [`migrate_link_syntax`] to the [`Migration::run`] signature.
fn run_link_syntax(
    project: &ProjectConfig,
    sources: &mut [Source],
    _doc: &mut toml_edit::DocumentMut,
    reporter: &mut Reporter,
) -> Result<()> {
    migrate_link_syntax(project, sources, reporter)
}

/// One rewritable occurrence of the removed `rheo-target` surface.
///
/// `rank` fixes the print/apply order to match the four forms below, so
/// output stays grouped exactly as the old sequential-regex passes grouped
/// it, even though a single tree walk finds all of them in document order.
struct TargetSite {
    rank: u8,
    range: Range<usize>,
    old: String,
    new: String,
}

/// `expr` is the `sys.inputs` field access (the shared receiver of every
/// `rheo-target` form below).
fn is_sys_inputs(expr: ast::Expr) -> bool {
    matches!(expr, ast::Expr::FieldAccess(fa)
        if matches!(fa.target(), ast::Expr::Ident(id) if id.as_str() == "sys")
            && fa.field().as_str() == "inputs")
}

/// `sys.inputs.at("rheo-target"[, default: <expr>])` -> `sys.inputs.rheo-context.at("target"[, default: <expr>])`.
///
/// The form the old regex missed entirely (it only matched a bare
/// `sys.inputs.rheo-target` field access). A `default:` argument is copied
/// verbatim by source span, since dropping a fallback would change what the
/// document compiles to. Any other second-argument shape (positional, or a
/// named argument other than `default`) is left unrewritten — the residual
/// `rheo-target` check below still flags it for manual fixing.
fn at_rewrite(
    source: &TypstSource,
    call_node: &SyntaxNode,
    call_offset: usize,
) -> Option<TargetSite> {
    let (args_node, args_offset) = args_of(call_node, call_offset)?;
    let items = args_items(args_node, args_offset);
    let (key_node, _) = items.first()?;
    if key_node.cast::<ast::Str>()?.get().as_str() != "rheo-target" {
        return None;
    }

    let (old, new) = match items.as_slice() {
        [_] => (
            "sys.inputs.at(\"rheo-target\")".to_string(),
            "sys.inputs.rheo-context.at(\"target\")".to_string(),
        ),
        [_, (named_node, named_range)] => {
            let named = named_node.cast::<ast::Named>()?;
            if named.name().as_str() != "default" {
                return None;
            }
            let default_text = &source.text()[named_range.clone()];
            (
                format!("sys.inputs.at(\"rheo-target\", {default_text})"),
                format!("sys.inputs.rheo-context.at(\"target\", {default_text})"),
            )
        }
        _ => return None,
    };

    Some(TargetSite {
        rank: 3,
        range: call_offset..call_offset + call_node.len(),
        old,
        new,
    })
}

impl SyntaxSite for TargetSite {
    fn visit(
        source: &TypstSource,
        node: &SyntaxNode,
        offset: usize,
        _ctx: WalkCtx,
        out: &mut Vec<Self>,
    ) {
        match node.kind() {
            SyntaxKind::FuncCall => {
                let Some(call) = node.cast::<ast::FuncCall>() else {
                    return;
                };
                match call.callee() {
                    // rheo-target() -> target()
                    ast::Expr::Ident(id)
                        if id.as_str() == "rheo-target"
                            && args_of(node, offset)
                                .is_some_and(|(a, o)| args_items(a, o).is_empty()) =>
                    {
                        out.push(TargetSite {
                            rank: 0,
                            range: offset..offset + node.len(),
                            old: "rheo-target()".to_string(),
                            new: "target()".to_string(),
                        });
                    }
                    // sys.inputs.at("rheo-target") -> sys.inputs.rheo-context.at("target")
                    ast::Expr::FieldAccess(fa)
                        if fa.field().as_str() == "at" && is_sys_inputs(fa.target()) =>
                    {
                        if let Some(site) = at_rewrite(source, node, offset) {
                            out.push(site);
                        }
                    }
                    _ => {}
                }
            }
            SyntaxKind::Binary => {
                let Some(bin) = node.cast::<ast::Binary>() else {
                    return;
                };
                // "rheo-target" in sys.inputs -> guarded rheo-context membership
                if bin.op() == ast::BinOp::In
                    && matches!(bin.lhs(), ast::Expr::Str(s) if s.get().as_str() == "rheo-target")
                    && is_sys_inputs(bin.rhs())
                {
                    out.push(TargetSite {
                        rank: 1,
                        range: offset..offset + node.len(),
                        old: "\"rheo-target\" in sys.inputs".to_string(),
                        new: "\"rheo-context\" in sys.inputs and \"target\" in sys.inputs.rheo-context".to_string(),
                    });
                }
            }
            SyntaxKind::FieldAccess => {
                let Some(fa) = node.cast::<ast::FieldAccess>() else {
                    return;
                };
                // sys.inputs.rheo-target -> sys.inputs.rheo-context.target
                if fa.field().as_str() == "rheo-target" && is_sys_inputs(fa.target()) {
                    out.push(TargetSite {
                        rank: 2,
                        range: offset..offset + node.len(),
                        old: "sys.inputs.rheo-target".to_string(),
                        new: "sys.inputs.rheo-context.target".to_string(),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Rewrite direct author references to the removed `sys.inputs.rheo-target` key
/// into the `sys.inputs.rheo-context.target` form, and calls to the removed
/// `rheo-target()` helper into Typst's polyfilled `target()`.
///
/// Four forms are handled (no rewrite's output contains another form's match
/// text, so applying all of a file's edits together, back-to-front, is
/// equivalent to running the old rules in sequence):
/// - `rheo-target()`                    -> `target()`
/// - `"rheo-target" in sys.inputs`      -> `"rheo-context" in sys.inputs and "target" in sys.inputs.rheo-context`
/// - `sys.inputs.rheo-target`           -> `sys.inputs.rheo-context.target`
/// - `sys.inputs.at("rheo-target", ..)` -> `sys.inputs.rheo-context.at("target", ..)`
///
/// Authors using the polyfilled `target()` need no change — it already reports
/// the output format. Any file still containing the literal `rheo-target`
/// afterwards (e.g. an `at(...)` call with an unsupported second argument) is
/// reported with a `warn!` for manual fixing.
fn migrate_target_references(sources: &mut [Source], reporter: &mut Reporter) -> Result<()> {
    if sources.is_empty() {
        return Ok(());
    }

    for source in sources.iter_mut() {
        let typ_source = TypstSource::detached(source.text.as_str());

        // One walk finds every form; `rank` (then document position) sorts the
        // print order back into the old per-form grouping.
        let mut sites = TargetSite::collect(&typ_source);
        sites.sort_by_key(|s| (s.rank, s.range.start));

        let mut content = source.text.clone();
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();
        for site in &sites {
            let line = line_number(&source.text, site.range.start);
            reporter.rewrite(&source.path, line, &site.old, &site.new);
            edits.push((site.range.clone(), site.new.clone()));
        }
        let changed = !edits.is_empty();
        if changed {
            apply_edits(&mut content, edits);
        }

        // Forms this migration does not auto-rewrite leave the literal behind.
        if content.contains("rheo-target") {
            warn!(
                file = %source.path.display(),
                "residual `rheo-target` reference remains after migration; hand-fix to `rheo-context.target`"
            );
        }

        if changed {
            source.text = content;
            source.dirty = true;
        }
    }

    Ok(())
}

/// Adapts [`migrate_target_references`] to the [`Migration::run`] signature.
fn run_target_references(
    _project: &ProjectConfig,
    sources: &mut [Source],
    _doc: &mut toml_edit::DocumentMut,
    reporter: &mut Reporter,
) -> Result<()> {
    migrate_target_references(sources, reporter)
}

/// The compatibility shim prepended to files that read the per-vertebra
/// `rheo-context` binding, which changed from a bare dict to a zero-arg function
/// `rheo-context()`.
const CONTEXT_SHIM: &str = "#let rheo-context = rheo-context()";

/// Whether `node`'s subtree contains an `Ident` reading the `rheo-context`
/// value binding, i.e. any `rheo-context` identifier except the `field` of a
/// `sys.inputs.rheo-context` field access (which reads the shared, unaffected
/// bundle-wide dict, not the per-file binding).
///
/// Typst tokenizes a hyphenated identifier as one token, so `rheo-context` is
/// never split across nodes — the label `<rheo-context>`, the ref
/// `@rheo-context`, a raw span, and a `"rheo-context"` string are their own
/// distinct `SyntaxKind`s (`Label`, `RefMarker`, `Raw`, `Str`), never `Ident`,
/// so they're excluded by construction rather than by an escape-char list.
/// The only case needing explicit exclusion — a `FieldAccess`'s `field`
/// child — has no representation in the shared walker's `WalkCtx` (which
/// tracks code/file-scope, not parent shape), so this walks the tree directly.
fn references_context_binding(node: &SyntaxNode) -> bool {
    if node.kind() == SyntaxKind::Ident && node.leaf_text().as_str() == "rheo-context" {
        return true;
    }
    if let Some(access) = node.cast::<ast::FieldAccess>() {
        let field = access.field().to_untyped();
        return node
            .children()
            .any(|child| !std::ptr::eq(child, field) && references_context_binding(child));
    }
    node.children().any(references_context_binding)
}

/// Keep authored files that read the per-vertebra `rheo-context` binding working
/// after it changed from a bare dict to a function `rheo-context()`.
///
/// Rather than rewrite every reference (a `rheo-context` identifier also appears
/// as a label `<rheo-context>`, a ref `@rheo-context`, a raw span, a `.rheo-context`
/// field, and a `"rheo-context"` string — splicing `()` into all those safely is
/// more than this migration needs), this prepends a one-line shim, `#let
/// rheo-context = rheo-context()`, to each file that reads the binding. The shim
/// calls the injected function once and rebinds the name to its dict, so existing
/// `rheo-context.field` / `ctx: rheo-context` code keeps working untouched.
///
/// [`references_context_binding`] only decides *whether* a file needs the shim.
fn migrate_context_references(sources: &mut [Source], reporter: &mut Reporter) -> Result<()> {
    if sources.is_empty() {
        return Ok(());
    }

    for source in sources.iter_mut() {
        // Already shimmed (e.g. a re-run) or no binding use: nothing to do.
        if source.text.contains(CONTEXT_SHIM) {
            continue;
        }
        let root = typst_syntax::parse(&source.text);
        if !references_context_binding(&root) {
            continue;
        }

        reporter.note(
            &source.path,
            "prepend `#let rheo-context = rheo-context()` compatibility shim",
        );

        source.text = format!("{CONTEXT_SHIM}\n{}", source.text);
        source.dirty = true;
    }

    Ok(())
}

/// Adapts [`migrate_context_references`] to the [`Migration::run`] signature.
fn run_context_references(
    _project: &ProjectConfig,
    sources: &mut [Source],
    _doc: &mut toml_edit::DocumentMut,
    reporter: &mut Reporter,
) -> Result<()> {
    migrate_context_references(sources, reporter)
}

/// A `#let <name> = ...` binding whose `<name>` is one of `REMOVED_VAR_BINDINGS`.
struct RemovedBindingSite {
    name: &'static str,
    replacement: &'static str,
    range: Range<usize>,
}

impl SyntaxSite for RemovedBindingSite {
    fn visit(
        _source: &TypstSource,
        node: &SyntaxNode,
        offset: usize,
        _ctx: WalkCtx,
        out: &mut Vec<Self>,
    ) {
        if node.kind() != SyntaxKind::LetBinding {
            return;
        }
        let Some(binding) = node.cast::<ast::LetBinding>() else {
            return;
        };
        let Some((name, replacement)) = binding.kind().bindings().into_iter().find_map(|ident| {
            REMOVED_VAR_BINDINGS
                .iter()
                .find(|(n, _)| *n == ident.as_str())
                .copied()
        }) else {
            return;
        };
        out.push(RemovedBindingSite {
            name,
            replacement,
            range: offset..offset + node.len(),
        });
    }
}

/// Reports (never rewrites) every removed `[html] feed_*` rheo.toml key and
/// `#let rheo-*` binding still present, each with its location and a pointer
/// to its replacement. A feed's title/author/base-url/inclusion rules do not
/// map one-to-one onto `@rheo/feeds`'s Typst configuration, so a mechanical
/// rewrite would produce something subtly wrong — this stays report-only.
fn report_removed_feed_surface(
    project: &ProjectConfig,
    sources: &[Source],
    reporter: &mut Reporter,
) {
    if let Some(html) = project.config.plugin_sections.get("html") {
        for retired in RETIRED_KEYS.iter().filter(|r| r.table == "[html]") {
            if html.extra.contains_key(retired.key) {
                reporter.retired("rheo.toml [html]", retired.key, retired.replacement);
            }
        }
    }

    for source in sources {
        let typ_source = TypstSource::detached(source.text.as_str());
        for site in RemovedBindingSite::collect(&typ_source) {
            let line = line_number(&source.text, site.range.start);
            reporter.retired(
                &format!("{}:{}", source.path.display(), line),
                site.name,
                site.replacement,
            );
        }
    }
}

/// Adapts [`report_removed_feed_surface`] to the [`Migration::run`] signature.
fn run_removed_feed_surface(
    project: &ProjectConfig,
    sources: &mut [Source],
    _doc: &mut toml_edit::DocumentMut,
    reporter: &mut Reporter,
) -> Result<()> {
    report_removed_feed_surface(project, sources, reporter);
    Ok(())
}

/// One `[spine]`/`[<plugin>.spine]` table that still sets the retired
/// `vertebrae` glob list.
struct VertebraeSite {
    /// `None` for the global `[spine]` table, `Some(plugin name)` for a
    /// per-format `[<plugin>.spine]` table.
    plugin: Option<String>,
    patterns: Vec<String>,
}

impl VertebraeSite {
    fn label(&self) -> String {
        match &self.plugin {
            Some(name) => format!("{name}.spine"),
            None => "spine".to_string(),
        }
    }
}

/// Read `spine.extra`'s `vertebrae` key (captured there since the typed field
/// was removed — see rheo-cyy) as a list of glob-pattern strings.
fn vertebrae_patterns(spine: &Spine) -> Option<Vec<String>> {
    let patterns = spine.extra.get("vertebrae")?.as_array()?;
    Some(
        patterns
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
    )
}

/// Expand `vertebrae` glob patterns into the `.typ` files they matched, the
/// same way the retired `SpineOptions::generate` used to (glob each pattern
/// against `content_dir`, keep only `.typ` files). Paths are canonicalized so
/// they compare equal to the directory-scan set regardless of path spelling.
///
/// Deliberately still the `glob` crate rather than `globset` (which core's
/// `CopyGlobs` and the spine excludes now share): this reproduces what the
/// *retired* `vertebrae` key actually matched, and `globset` additionally
/// understands brace alternation (`*.{png,jpg}`) that `glob` doesn't — porting
/// the engine here could change what an old project's `vertebrae` list
/// expands to, silently changing the `exclude` this migration computes for it.
fn expand_vertebrae_patterns(patterns: &[String], content_dir: &Path) -> HashSet<PathBuf> {
    let mut matched = HashSet::new();
    for pattern in patterns {
        let glob_pattern = content_dir.join(pattern).display().to_string();
        let paths = match glob::glob(&glob_pattern) {
            Ok(paths) => paths,
            Err(e) => {
                warn!(pattern = %pattern, error = %e, "invalid vertebrae glob pattern; skipping");
                continue;
            }
        };
        for path in paths.filter_map(|p| p.ok()) {
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("typ") {
                matched.insert(canonicalize_path(&path).unwrap_or(path));
            }
        }
    }
    matched
}

/// Convert a retired `vertebrae` inclusion-filter glob list into an equivalent
/// `[spine] exclude`, so a helper-only `.typ` file the old list deliberately
/// never named doesn't silently start being published as a spine page under
/// the directory-scan-by-default model (see rheo-9vl.1). When `vertebrae`
/// matched every `.typ` file anyway (e.g. an empty list, or a catch-all glob),
/// no exclude is needed — the key is simply dropped.
fn migrate_vertebrae_to_exclude(
    project: &ProjectConfig,
    _sources: &mut [Source],
    doc: &mut toml_edit::DocumentMut,
    reporter: &mut Reporter,
) -> Result<()> {
    let mut sites = Vec::new();
    if let Some(spine) = &project.config.spine
        && let Some(patterns) = vertebrae_patterns(spine)
    {
        sites.push(VertebraeSite {
            plugin: None,
            patterns,
        });
    }
    for (name, section) in &project.config.plugin_sections {
        if let Some(spine) = &section.spine
            && let Some(patterns) = vertebrae_patterns(spine)
        {
            sites.push(VertebraeSite {
                plugin: Some(name.clone()),
                patterns,
            });
        }
    }
    if sites.is_empty() {
        return Ok(());
    }
    // Sort for deterministic output across runs (HashMap iteration order varies).
    sites.sort_by_key(|a| a.label());

    let content_dir = resolve_effective_content_dir(project);
    // Full directory-scan file set the new zero-config model would include.
    // No `.typ` files under content_dir means nothing to reconcile.
    let Ok(scan) = SpineScan::run(&content_dir, &[]) else {
        return Ok(());
    };
    let scanned: HashSet<PathBuf> = scan
        .files
        .into_iter()
        .map(|f| canonicalize_path(&f).unwrap_or(f))
        .collect();

    for site in &sites {
        // An empty pattern list means "auto-discover everything" under the old
        // semantics too — already equivalent to the new default, no diff.
        let newly_included: Vec<String> = if site.patterns.is_empty() {
            Vec::new()
        } else {
            let matched = expand_vertebrae_patterns(&site.patterns, &content_dir);
            let mut rel: Vec<String> = scanned
                .iter()
                .filter(|f| !matched.contains(*f))
                .map(|f| to_forward_slash(f.strip_prefix(&content_dir).unwrap_or(f)))
                .collect();
            rel.sort();
            rel
        };

        if newly_included.is_empty() {
            reporter.line(format_args!(
                "  - [{}]: vertebrae matched the full scan; removing (no exclude needed)",
                site.label()
            ));
        } else {
            reporter.line(format_args!(
                "  - [{}]: vertebrae -> exclude = {:?}",
                site.label(),
                newly_included
            ));
        }

        let item = match &site.plugin {
            Some(name) => &mut doc[name.as_str()]["spine"],
            None => &mut doc["spine"],
        };
        if let Some(table) = item.as_table_like_mut() {
            table.remove("vertebrae");
            if !newly_included.is_empty() {
                match table.get_mut("exclude").and_then(|i| i.as_array_mut()) {
                    Some(arr) => {
                        for f in &newly_included {
                            if !arr.iter().any(|v| v.as_str() == Some(f.as_str())) {
                                arr.push(f.as_str());
                            }
                        }
                    }
                    None => {
                        let mut arr = toml_edit::Array::new();
                        for f in &newly_included {
                            arr.push(f.as_str());
                        }
                        table.insert(
                            "exclude",
                            toml_edit::Item::Value(toml_edit::Value::Array(arr)),
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

/// Locate a `FuncCall` node's `Args` child and its absolute byte offset,
/// given the call's own offset (as supplied by the tree walker).
fn args_of(call_node: &SyntaxNode, call_offset: usize) -> Option<(&SyntaxNode, usize)> {
    let mut offset = call_offset;
    for child in call_node.children() {
        if child.kind() == SyntaxKind::Args {
            return Some((child, offset));
        }
        offset += child.len();
    }
    None
}

/// Every "real" (non-trivia, non-punctuation) child of an `Args` node, each
/// paired with its absolute byte range — `args_offset` is the `Args` node's
/// own offset, from [`args_of`].
fn args_items(args_node: &SyntaxNode, args_offset: usize) -> Vec<(&SyntaxNode, Range<usize>)> {
    let mut offset = args_offset;
    let mut items = Vec::new();
    for child in args_node.children() {
        if !child.kind().is_trivia()
            && !matches!(
                child.kind(),
                SyntaxKind::LeftParen | SyntaxKind::RightParen | SyntaxKind::Comma
            )
        {
            items.push((child, offset..offset + child.len()));
        }
        offset += child.len();
    }
    items
}

/// If `call_node`'s first argument is a `Str` ending `.typ`, its unescaped
/// text and byte range (of the `Str` token alone, quotes included).
fn first_str_arg(call_node: &SyntaxNode, call_offset: usize) -> Option<(String, Range<usize>)> {
    let (args_node, args_offset) = args_of(call_node, call_offset)?;
    let (node, range) = args_items(args_node, args_offset).into_iter().next()?;
    let text = node.cast::<ast::Str>()?.get().to_string();
    text.ends_with(".typ").then_some((text, range))
}

/// Splice `edits` into `text`, back-to-front (highest byte offset first) so
/// each replacement leaves every not-yet-applied range's offsets valid.
fn apply_edits(text: &mut String, mut edits: Vec<(Range<usize>, String)>) {
    edits.sort_by_key(|(range, _)| range.start);
    for (range, replacement) in edits.into_iter().rev() {
        text.replace_range(range, &replacement);
    }
}

/// Resolve a `.typ` link href to an absolute path.
///
/// `/`-prefixed hrefs are resolved against the content directory (Typst's root);
/// relative hrefs (including `./`) are resolved against the linking file's
/// directory.
fn resolve_href(href: &str, file_dir: &Path, content_dir: &Path) -> PathBuf {
    let p = Path::new(href);
    if p.is_absolute() {
        content_dir.join(p.strip_prefix("/").unwrap_or(p))
    } else {
        file_dir.join(p)
    }
}

/// Collect every `.typ` file beneath `dir`, sorted lexicographically.
fn collect_typ_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("typ"))
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();
    files
}

/// 1-based line number of the given byte offset within `text`.
fn line_number(text: &str, byte_offset: usize) -> usize {
    text[..byte_offset.min(text.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

/// Set the top-level `version` key on an already-parsed document, preserving
/// all other formatting (the reason `toml_edit` is used over a serde
/// round-trip, which would drop comments and reformat the file).
fn bump_version(doc: &mut toml_edit::DocumentMut, target: &ManifestVersion) {
    doc["version"] = toml_edit::value(target.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Write every `dirty` source to disk — what `migrate_project` does in one
    /// pass after all due migrations have run. Tests below call a single
    /// migration directly, so they flush by hand to inspect the result on disk.
    fn flush_sources(sources: &[Source]) {
        for source in sources {
            if source.dirty {
                fs::write(&source.path, source.text.as_bytes()).unwrap();
            }
        }
    }

    #[test]
    fn manifest_version_orders() {
        let old = ManifestVersion::parse("0.3.0").unwrap();
        let new = ManifestVersion::parse("0.4.0").unwrap();
        assert!(old < new);
        assert!(new > old);
    }

    #[test]
    fn migrations_since_versions_parse() {
        for m in MIGRATIONS {
            assert!(
                ManifestVersion::parse(m.since).is_ok(),
                "MIGRATIONS entry {:?} has an unparseable `since`",
                m.plan
            );
        }
    }

    #[test]
    fn bump_version_preserves_formatting() {
        let original = "# a leading comment\nversion = \"0.3.0\"\ncontent_dir = \"pages\"\n\n[pdf.spine]\nvertebrae = [\"a.typ\"]\ntitle = \"Book\"\n";
        let mut doc: toml_edit::DocumentMut = original.parse().unwrap();

        let target = ManifestVersion::parse("0.4.0").unwrap();
        bump_version(&mut doc, &target);

        let updated = doc.to_string();
        assert!(updated.starts_with("# a leading comment\n"));
        assert!(updated.contains("content_dir = \"pages\""));
        assert!(updated.contains("title = \"Book\""));
        assert!(updated.contains("version = \"0.4.0\""));
        assert!(!updated.contains("0.3.0"));
    }

    #[test]
    fn resolve_href_relative_and_rooted() {
        let file_dir = Path::new("/proj/content/ch1");
        let content_dir = Path::new("/proj/content");

        // Relative to the linking file's directory.
        let rel = resolve_href("./sibling.typ", file_dir, content_dir);
        assert_eq!(rel, PathBuf::from("/proj/content/ch1/sibling.typ"));

        // Rooted at the content directory.
        let root = resolve_href("/intro.typ", file_dir, content_dir);
        assert_eq!(root, PathBuf::from("/proj/content/intro.typ"));
    }

    /// A dry run's whole user-facing report is assertable, which is the point
    /// of routing it through the reporter: the version gap, the plan, each
    /// rewrite, and the closing dry-run notice.
    #[test]
    fn dry_run_reports_the_plan_the_rewrites_and_that_nothing_was_written() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("about.typ"), "= About\n").unwrap();
        fs::write(content.join("intro.typ"), "#link(\"./about.typ\")[About]\n").unwrap();
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        let (mut reporter, captured) = Reporter::capture();
        migrate_project(root, false, &mut reporter).unwrap();
        let report = captured.text();

        assert!(report.contains("Project version: 0.3.0"), "{report}");
        assert!(
            report.contains("  - rewrite #link(\"./file.typ\") syntax to #link(<handle>)"),
            "{report}"
        );
        assert!(
            report.contains(": #link(\"./about.typ\")  ->  #link(<about>)"),
            "{report}"
        );
        assert!(
            report.contains("Dry run; no changes made. Re-run with --apply to write them."),
            "{report}"
        );
        // A dry run writes nothing, whatever it reported.
        assert!(
            fs::read_to_string(content.join("intro.typ"))
                .unwrap()
                .contains("#link(\"./about.typ\")"),
            "dry run must not rewrite the file"
        );
    }

    #[test]
    fn rewrite_replaces_old_link_syntax() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();
        fs::write(content.join("about.typ"), "= About\n").unwrap();
        // intro links to about with old syntax; an external URL is left alone.
        fs::write(
            content.join("intro.typ"),
            "#link(\"./about.typ\")[About]\n#link(\"https://example.com\")[ex]\n",
        )
        .unwrap();

        let project = ProjectConfig {
            root: root.to_path_buf(),
            name: "test".into(),
            config: rheo_core::RheoConfig {
                version: ManifestVersion::parse("0.3.0").unwrap(),
                content_dir: Some("content".into()),
                ..Default::default()
            },
            typ_files: vec![content.join("intro.typ"), content.join("about.typ")],
            mode: rheo_core::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        let mut sources = load_sources(&content);
        migrate_link_syntax(&project, &mut sources, &mut Reporter::capture().0).unwrap();
        flush_sources(&sources);

        let rewritten = fs::read_to_string(content.join("intro.typ")).unwrap();
        assert!(rewritten.contains("#link(<about>)[About]"));
        // External URL untouched.
        assert!(rewritten.contains("#link(\"https://example.com\")[ex]"));
    }

    #[test]
    fn rewrite_ignores_link_syntax_in_comments_and_raw_spans() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("about.typ"), "= About\n").unwrap();
        // Three demonstrated false positives for the old regex (a `//` line
        // comment, a `/* */` block comment, a raw span), plus a real link on
        // its own line that must still be rewritten.
        fs::write(
            content.join("intro.typ"),
            "= Intro\n\
             // #link(\"./about.typ\")[commented out]\n\
             /* #link(\"./about.typ\")[also commented out] */\n\
             `#link(\"./about.typ\")[raw, not code]`\n\
             #link(\"./about.typ\")[Real link]\n",
        )
        .unwrap();

        let project = ProjectConfig {
            root: root.to_path_buf(),
            name: "test".into(),
            config: rheo_core::RheoConfig {
                version: ManifestVersion::parse("0.3.0").unwrap(),
                content_dir: Some("content".into()),
                ..Default::default()
            },
            typ_files: vec![content.join("intro.typ"), content.join("about.typ")],
            mode: rheo_core::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        let mut sources = load_sources(&content);
        migrate_link_syntax(&project, &mut sources, &mut Reporter::capture().0).unwrap();
        flush_sources(&sources);

        let rewritten = fs::read_to_string(content.join("intro.typ")).unwrap();
        // The three false positives are left exactly as written.
        assert!(
            rewritten.contains("// #link(\"./about.typ\")[commented out]"),
            "line comment must be untouched:\n{rewritten}"
        );
        assert!(
            rewritten.contains("/* #link(\"./about.typ\")[also commented out] */"),
            "block comment must be untouched:\n{rewritten}"
        );
        assert!(
            rewritten.contains("`#link(\"./about.typ\")[raw, not code]`"),
            "raw span must be untouched:\n{rewritten}"
        );
        // The real link is still rewritten.
        assert!(
            rewritten.contains("#link(<about>)[Real link]"),
            "real link must be rewritten:\n{rewritten}"
        );
    }

    #[test]
    fn rewrite_replaces_target_references() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(
            content.join("page.typ"),
            "#let fmt = rheo-target()\n\
             #if \"rheo-target\" in sys.inputs { sys.inputs.rheo-target }\n\
             = Unrelated Title\n",
        )
        .unwrap();

        fs::write(
            root.join("rheo.toml"),
            "version = \"0.4.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        let mut sources = load_sources(&content);
        migrate_target_references(&mut sources, &mut Reporter::capture().0).unwrap();
        flush_sources(&sources);

        let out = fs::read_to_string(content.join("page.typ")).unwrap();
        // Helper call -> polyfilled target().
        assert!(out.contains("#let fmt = target()"));
        // Membership guard rewritten.
        assert!(
            out.contains(
                "\"rheo-context\" in sys.inputs and \"target\" in sys.inputs.rheo-context"
            )
        );
        // Value access rewritten.
        assert!(out.contains("{ sys.inputs.rheo-context.target }"));
        // No trace of the removed key/helper remains.
        assert!(!out.contains("rheo-target"));
        // Unrelated content untouched.
        assert!(out.contains("= Unrelated Title"));
    }

    #[test]
    fn shims_files_that_read_the_context_binding() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // A file that reads the binding, alongside a label, a ref, a raw span,
        // the global dict, and the detection string — none of which must change.
        fs::write(
            content.join("page.typ"),
            "#show: t.with(ctx: rheo-context)\n\
             This is #rheo-context.handle of #rheo-context.spine-flat.len() pages.\n\
             #if \"rheo-context\" in sys.inputs { sys.inputs.rheo-context.spine }\n\
             See #link(<rheo-context>)[the page], @rheo-context, and `rheo-context.target`.\n\
             = Unrelated Title\n",
        )
        .unwrap();
        // A file that only links to / mentions the handle (label + global dict),
        // never reading the binding value — must NOT be shimmed.
        fs::write(
            content.join("link_only.typ"),
            "See #link(<rheo-context>)[the context page]; sys.inputs.rheo-context.spine.\n",
        )
        .unwrap();

        fs::write(
            root.join("rheo.toml"),
            "version = \"0.5.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        let mut sources = load_sources(&content);
        migrate_context_references(&mut sources, &mut Reporter::capture().0).unwrap();
        flush_sources(&sources);

        let out = fs::read_to_string(content.join("page.typ")).unwrap();
        // The shim is prepended once, ahead of the body.
        assert!(out.starts_with("#let rheo-context = rheo-context()\n"));
        assert_eq!(out.matches("#let rheo-context = rheo-context()").count(), 1);
        // Every original reference is left exactly as written — nothing rewritten.
        assert!(out.contains("#show: t.with(ctx: rheo-context)\n"));
        assert!(out.contains("#rheo-context.handle of #rheo-context.spine-flat.len()"));
        assert!(out.contains("\"rheo-context\" in sys.inputs"));
        assert!(out.contains("sys.inputs.rheo-context.spine"));
        assert!(out.contains("#link(<rheo-context>)"));
        assert!(out.contains("@rheo-context,"));
        assert!(out.contains("`rheo-context.target`"));
        assert!(out.contains("= Unrelated Title"));

        // The link-only file reads no binding value, so it is left untouched.
        let link_only = fs::read_to_string(content.join("link_only.typ")).unwrap();
        assert!(!link_only.contains(CONTEXT_SHIM));
    }

    #[test]
    fn vertebrae_migration_excludes_files_the_old_list_never_named() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = root.join("content");
        fs::create_dir_all(content.join("lib")).unwrap();
        fs::write(content.join("main.typ"), "= Main\n").unwrap();
        fs::write(content.join("lib").join("helper.typ"), "#let x = 1\n").unwrap();

        let mut extra = toml::Table::new();
        extra.insert(
            "vertebrae".to_string(),
            toml::Value::Array(vec![toml::Value::String("main.typ".to_string())]),
        );
        let spine = Spine {
            title: Some("Book".to_string()),
            extra,
            ..Default::default()
        };
        let mut plugin_sections = HashMap::new();
        plugin_sections.insert(
            "epub".to_string(),
            rheo_core::PluginSection {
                spine: Some(spine),
                ..Default::default()
            },
        );

        let project = ProjectConfig {
            root: root.to_path_buf(),
            name: "test".into(),
            config: rheo_core::RheoConfig {
                version: ManifestVersion::parse("0.3.0").unwrap(),
                content_dir: Some("content".into()),
                plugin_sections,
                ..Default::default()
            },
            typ_files: vec![
                content.join("main.typ"),
                content.join("lib").join("helper.typ"),
            ],
            mode: rheo_core::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        let toml_path = root.join("rheo.toml");
        fs::write(
            &toml_path,
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n\n[epub.spine]\ntitle = \"Book\"\nvertebrae = [\"main.typ\"]\n",
        )
        .unwrap();

        let mut doc = load_toml_doc(&toml_path).unwrap();
        migrate_vertebrae_to_exclude(&project, &mut [], &mut doc, &mut Reporter::capture().0)
            .unwrap();
        let updated = doc.to_string();
        assert!(!updated.contains("vertebrae"), "{updated}");
        assert!(updated.contains("exclude"), "{updated}");
        assert!(updated.contains("lib/helper.typ"), "{updated}");
        // Unrelated keys preserved.
        assert!(updated.contains("title = \"Book\""), "{updated}");
    }

    #[test]
    fn vertebrae_migration_drops_key_with_no_exclude_when_nothing_new_included() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("main.typ"), "= Main\n").unwrap();

        let mut extra = toml::Table::new();
        extra.insert(
            "vertebrae".to_string(),
            toml::Value::Array(vec![toml::Value::String("*.typ".to_string())]),
        );
        let spine = Spine {
            extra,
            ..Default::default()
        };
        let mut plugin_sections = HashMap::new();
        plugin_sections.insert(
            "pdf".to_string(),
            rheo_core::PluginSection {
                spine: Some(spine),
                ..Default::default()
            },
        );

        let project = ProjectConfig {
            root: root.to_path_buf(),
            name: "test".into(),
            config: rheo_core::RheoConfig {
                version: ManifestVersion::parse("0.3.0").unwrap(),
                content_dir: Some("content".into()),
                plugin_sections,
                ..Default::default()
            },
            typ_files: vec![content.join("main.typ")],
            mode: rheo_core::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        let toml_path = root.join("rheo.toml");
        fs::write(
            &toml_path,
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n\n[pdf.spine]\nvertebrae = [\"*.typ\"]\n",
        )
        .unwrap();

        let mut doc = load_toml_doc(&toml_path).unwrap();
        migrate_vertebrae_to_exclude(&project, &mut [], &mut doc, &mut Reporter::capture().0)
            .unwrap();
        let updated = doc.to_string();
        assert!(!updated.contains("vertebrae"), "{updated}");
        assert!(!updated.contains("exclude"), "{updated}");
    }

    #[test]
    fn rewrite_uses_path_qualified_handle_on_collision() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = root.join("content");
        let sub = content.join("chapters");
        fs::create_dir_all(&sub).unwrap();
        // Two files share the stem "intro" -> collision.
        fs::write(content.join("intro.typ"), "= root\n").unwrap();
        fs::write(
            sub.join("intro.typ"),
            "#link(\"../intro.typ\")[root intro]\n",
        )
        .unwrap();

        let project = ProjectConfig {
            root: root.to_path_buf(),
            name: "test".into(),
            config: rheo_core::RheoConfig {
                version: ManifestVersion::parse("0.3.0").unwrap(),
                content_dir: Some("content".into()),
                ..Default::default()
            },
            typ_files: vec![content.join("intro.typ"), sub.join("intro.typ")],
            mode: rheo_core::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        let mut sources = load_sources(&content);
        migrate_link_syntax(&project, &mut sources, &mut Reporter::capture().0).unwrap();
        flush_sources(&sources);

        let rewritten = fs::read_to_string(sub.join("intro.typ")).unwrap();
        // The root file's primary handle stays bare `intro` even under collision
        // (its rel_stem has no path prefix), so the link targets `<intro>`.
        assert!(rewritten.contains("#link(<intro>)[root intro]"));
    }

    #[test]
    fn rewrite_uses_path_qualified_handle_for_nested_collision_member() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let content = root.join("content");
        let sub = content.join("chapters");
        fs::create_dir_all(&sub).unwrap();
        // Two files share the stem "intro" -> collision.
        fs::write(
            content.join("intro.typ"),
            "#link(\"./chapters/intro.typ\")[nested]\n",
        )
        .unwrap();
        fs::write(sub.join("intro.typ"), "= nested\n").unwrap();

        let project = ProjectConfig {
            root: root.to_path_buf(),
            name: "test".into(),
            config: rheo_core::RheoConfig {
                version: ManifestVersion::parse("0.3.0").unwrap(),
                content_dir: Some("content".into()),
                ..Default::default()
            },
            typ_files: vec![content.join("intro.typ"), sub.join("intro.typ")],
            mode: rheo_core::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        let mut sources = load_sources(&content);
        migrate_link_syntax(&project, &mut sources, &mut Reporter::capture().0).unwrap();
        flush_sources(&sources);

        let rewritten = fs::read_to_string(content.join("intro.typ")).unwrap();
        // Nested collision member -> path-qualified primary handle `chapters:intro`,
        // never the ambiguous escape form `<intro.typ>`.
        assert!(rewritten.contains("#link(<chapters:intro>)[nested]"));
        assert!(!rewritten.contains("<intro.typ>"));
    }
}

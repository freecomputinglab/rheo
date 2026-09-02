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

use regex::{Captures, Regex};
use rheo_core::build::resolve_effective_content_dir;
use rheo_core::config::RETIRED_KEYS;
use rheo_core::config::manifest_version::ManifestVersion;
use rheo_core::config::project::ProjectConfig;
use rheo_core::reticulate::{SpineLayout, SpineScan, VirtualSpine};
use rheo_core::util::path::{canonicalize_path, to_forward_slash};
use rheo_core::{Result, RheoError, Spine};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use walkdir::WalkDir;

/// Version at which the `#link("./file.typ")` syntax was replaced by the
/// `#link(<handle>)` label syntax. Projects older than this need a link rewrite.
const LINK_SYNTAX_VERSION: &str = "0.4.0";

/// Version at which `sys.inputs.rheo-target` and the `rheo-target()` helper were
/// removed in favour of `sys.inputs.rheo-context.target` / the polyfilled
/// `target()`. Projects older than this have their direct references rewritten.
const TARGET_REMOVED_VERSION: &str = "0.5.0";

/// Version at which the `[spine] vertebrae` inclusion-filter glob list was retired
/// by the directory-scan default. Projects older than this have any `vertebrae`
/// list converted to an equivalent `exclude`.
const VERTEBRAE_RETIRED_VERSION: &str = "0.5.0";

/// Version at which the per-vertebra `rheo-context` binding changed from a bare
/// dict to a zero-arg function `rheo-context()`. Projects older than this have a
/// compatibility shim prepended to each file that reads the binding.
const CONTEXT_FN_VERSION: &str = "0.5.1";

/// Version at which the Rust Atom feed generator, its `[html] feed_*`
/// rheo.toml keys, and the `rheo-*` `.typ` variable convention were removed.
/// Projects older than this have every affected key/binding REPORTED (see
/// `report_removed_feed_surface`) — not rewritten.
const FEED_REMOVED_VERSION: &str = "0.6.0";

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

/// Run migration for the project at `path`.
///
/// `apply == false` is a dry run: it reports the version gap, prints each link
/// that would be rewritten, but writes nothing. `apply == true` rewrites links
/// and bumps the `version` field in `rheo.toml`.
pub fn migrate_project(path: &Path, apply: bool) -> Result<()> {
    info!(path = %path.display(), "loading project for migration");
    let project = ProjectConfig::from_path(path, None)?;

    let config_path = project.config_path.as_ref().ok_or_else(|| {
        RheoError::project_config("no rheo.toml found for this project; nothing to migrate")
    })?;

    let from = project.config.version.clone();
    let to = ManifestVersion::current();

    info!(from = %from, to = %to, "migration target");
    println!("Project version: {from}");
    println!("Target version:  {to}");

    if from >= to {
        println!("Project is already up to date; nothing to migrate.");
        return Ok(());
    }

    // Each migration is gated on the version at which its change actually landed,
    // so a patch/minor bump only runs the migrations introduced since the
    // project's recorded version — not every historical migration on every bump.
    let threshold =
        |v: &str| ManifestVersion::parse(v).expect("hardcoded threshold must be valid semver");
    let needs_link_rewrite = from < threshold(LINK_SYNTAX_VERSION);
    let needs_target_rewrite = from < threshold(TARGET_REMOVED_VERSION);
    let needs_vertebrae_migration = from < threshold(VERTEBRAE_RETIRED_VERSION);
    let needs_context_rewrite = from < threshold(CONTEXT_FN_VERSION);
    let needs_feed_report = from < threshold(FEED_REMOVED_VERSION);

    println!("\nMigrations:");
    if needs_link_rewrite {
        println!("  - rewrite #link(\"./file.typ\") syntax to #link(<handle>)");
    }
    if needs_target_rewrite {
        println!(
            "  - rewrite sys.inputs.rheo-target -> sys.inputs.rheo-context.target (and rheo-target() -> target())"
        );
    }
    if needs_vertebrae_migration {
        println!("  - convert retired [spine] vertebrae glob lists to [spine] exclude");
    }
    if needs_context_rewrite {
        println!("  - shim `rheo-context` binding for its new `rheo-context()` function form");
    }
    if needs_feed_report {
        println!("  - report removed Atom feed config keys and rheo-* variables (no rewrite)");
    }
    println!("  - bump rheo.toml version to {to}");

    if needs_link_rewrite {
        println!("\nLink rewrites:");
        migrate_link_syntax(&project, apply)?;
    }

    if needs_target_rewrite {
        println!("\nTarget references:");
        migrate_target_references(&project, apply)?;
    }

    if needs_vertebrae_migration {
        println!("\nSpine config:");
        migrate_vertebrae_to_exclude(&project, config_path, apply)?;
    }

    if needs_context_rewrite {
        println!("\nContext references:");
        migrate_context_references(&project, apply)?;
    }

    if needs_feed_report {
        println!("\nRemoved feed surface:");
        report_removed_feed_surface(&project);
    }

    if apply {
        bump_version(config_path, &to)?;
        println!("\nBumped rheo.toml version to {to}.");
    } else {
        println!("\nDry run; no changes made. Re-run with --apply to write them.");
    }
    Ok(())
}

/// Rewrite old `#link("./file.typ")` syntax to the `#link(<handle>)` form.
///
/// Handles are taken from `VirtualSpine::build` (`crates/core/src/reticulate/
/// spine.rs`), which is collision-aware: the primary handle is bare (`<intro>`)
/// when the stem is unique, and path-qualified with `:` separator (`<chapters:intro>`)
/// for nested files. The `<stem.typ>` escape alias is ambiguous when stems collide,
/// so it is never used as a rewrite target.
fn migrate_link_syntax(project: &ProjectConfig, apply: bool) -> Result<()> {
    let content_dir = resolve_effective_content_dir(project);
    let typ_files = collect_typ_files(&content_dir);
    if typ_files.is_empty() {
        return Ok(());
    }

    // Build the handle map over every source file so any `.typ` target resolves.
    let layout = SpineLayout::OnePerVertebra {
        ext: "html".into(),
        format: "html".into(),
    };
    let spine = VirtualSpine::build(
        SpineScan::flat(&typ_files, &content_dir),
        &project.root,
        layout,
    )?;

    // Canonical absolute source path -> label to emit.
    // The primary handle is always unique: bare (`intro`) for root-level files,
    // path-qualified with ':' (`chapters:intro`) for nested files.
    // The `<stem.typ>` escape alias is basename-based and AMBIGUOUS on
    // stem collision, so it is never used as a rewrite target.
    let mut handle_map: HashMap<PathBuf, String> = HashMap::new();
    for v in &spine.vertebrae {
        let abs = project.root.join(&v.rel_path);
        let canon = canonicalize_path(&abs).unwrap_or(abs);
        handle_map.insert(canon, v.handle.clone());
    }

    let re = Regex::new(r##"#link\("([^"]*\.typ)"\)"##).expect("hardcoded link regex must compile");
    for file in &typ_files {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                warn!(file = %file.display(), error = %e, "skipping unreadable file");
                continue;
            }
        };
        let parent = file.parent().unwrap_or_else(|| Path::new(""));
        let mut changed = false;

        let rewritten = re.replace_all(&content, |caps: &Captures| -> String {
            let href = &caps[1];
            // Leave external URLs untouched.
            if href.contains("://") {
                return caps[0].to_string();
            }
            let resolved = resolve_href(href, parent, &content_dir);
            match resolved
                .and_then(|p| canonicalize_path(&p).ok())
                .and_then(|c| handle_map.get(&c).cloned())
            {
                Some(target) => {
                    let line = line_number(&content, caps.get(0).unwrap().start());
                    info!(file = %file.display(), line, old = href, new = %target, "rewrite link");
                    println!(
                        "{}:{}: #link(\"{}\")  ->  #link(<{}>)",
                        file.display(),
                        line,
                        href,
                        target
                    );
                    changed = true;
                    format!("#link(<{target}>)")
                }
                None => {
                    warn!(file = %file.display(), href, "link target is not a vertebra; skipping");
                    caps[0].to_string()
                }
            }
        });

        if changed && apply {
            fs::write(file, rewritten.as_bytes())
                .map_err(|e| RheoError::io(e, format!("writing {}", file.display())))?;
        }
    }

    Ok(())
}

/// Rewrite direct author references to the removed `sys.inputs.rheo-target` key
/// into the `sys.inputs.rheo-context.target` form, and calls to the removed
/// `rheo-target()` helper into Typst's polyfilled `target()`.
///
/// Three textual forms are handled, in order (no rewrite's output contains
/// another rule's match text, so the passes are independent):
/// - `rheo-target()`               -> `target()`
/// - `"rheo-target" in sys.inputs` -> `"rheo-context" in sys.inputs and "target" in sys.inputs.rheo-context`
/// - `sys.inputs.rheo-target`      -> `sys.inputs.rheo-context.target`
///
/// Authors using the polyfilled `target()` need no change — it already reports
/// the output format. Any file still
/// containing the literal `rheo-target` afterwards (e.g. a
/// `sys.inputs.at("rheo-target")` form this does not rewrite) is reported with a
/// `warn!` for manual fixing.
fn migrate_target_references(project: &ProjectConfig, apply: bool) -> Result<()> {
    let content_dir = resolve_effective_content_dir(project);
    let typ_files = collect_typ_files(&content_dir);
    if typ_files.is_empty() {
        return Ok(());
    }

    let rules: [(Regex, &str, &str); 3] = [
        (
            Regex::new(r"rheo-target\(\s*\)").expect("hardcoded regex must compile"),
            "target()",
            "rheo-target()  ->  target()",
        ),
        (
            Regex::new(r#""rheo-target"\s+in\s+sys\.inputs"#)
                .expect("hardcoded regex must compile"),
            r#""rheo-context" in sys.inputs and "target" in sys.inputs.rheo-context"#,
            "\"rheo-target\" in sys.inputs  ->  \"rheo-context\" in sys.inputs and \"target\" in sys.inputs.rheo-context",
        ),
        (
            Regex::new(r"sys\.inputs\.rheo-target").expect("hardcoded regex must compile"),
            "sys.inputs.rheo-context.target",
            "sys.inputs.rheo-target  ->  sys.inputs.rheo-context.target",
        ),
    ];

    for file in &typ_files {
        let mut content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                warn!(file = %file.display(), error = %e, "skipping unreadable file");
                continue;
            }
        };
        let mut changed = false;

        for (re, replacement, label) in &rules {
            let mut hit = false;
            let out = re.replace_all(&content, |caps: &Captures| -> String {
                let line = line_number(&content, caps.get(0).unwrap().start());
                info!(file = %file.display(), line, rewrite = label, "rewrite target reference");
                println!("{}:{}: {}", file.display(), line, label);
                hit = true;
                (*replacement).to_string()
            });
            if hit {
                content = out.into_owned();
                changed = true;
            }
        }

        // Forms this migration does not auto-rewrite leave the literal behind.
        if content.contains("rheo-target") {
            warn!(
                file = %file.display(),
                "residual `rheo-target` reference remains after migration; hand-fix to `rheo-context.target`"
            );
        }

        if changed && apply {
            fs::write(file, content.as_bytes())
                .map_err(|e| RheoError::io(e, format!("writing {}", file.display())))?;
        }
    }

    Ok(())
}

/// The compatibility shim prepended to files that read the per-vertebra
/// `rheo-context` binding, which changed from a bare dict to a zero-arg function
/// `rheo-context()`.
const CONTEXT_SHIM: &str = "#let rheo-context = rheo-context()";

/// Keep authored files that read the per-vertebra `rheo-context` binding working
/// after it changed from a bare dict to a function `rheo-context()`.
///
/// Rather than rewrite every reference (a `rheo-context` identifier also appears
/// as a label `<rheo-context>`, a ref `@rheo-context`, a raw span, a `.rheo-context`
/// field, and a `"rheo-context"` string — regex can't safely splice `()` into all
/// those), this prepends a one-line shim, `#let rheo-context = rheo-context()`,
/// to each file that reads the binding. The shim calls the injected function once
/// and rebinds the name to its dict, so existing `rheo-context.field` /
/// `ctx: rheo-context` code keeps working untouched.
///
/// The detection regex only decides *whether* a file needs the shim (a false
/// positive would add a harmless extra shim, so it errs toward precision without
/// needing to be exact). It matches a `rheo-context` value reference: `pre`
/// excludes the lead-in chars of the non-binding forms — `.` (`sys.inputs.rheo-context`
/// field), `"` (string), `<` (label), `@` (ref), a backtick (raw span), and word/`-`
/// (a longer identifier); `post` excludes `(` (already a call) and word/`-`.
fn migrate_context_references(project: &ProjectConfig, apply: bool) -> Result<()> {
    let content_dir = resolve_effective_content_dir(project);
    let typ_files = collect_typ_files(&content_dir);
    if typ_files.is_empty() {
        return Ok(());
    }

    let re = Regex::new(r#"(?:^|[^.\w"<@`-])rheo-context(?:$|[^\w(-])"#)
        .expect("hardcoded context regex must compile");

    for file in &typ_files {
        let content = match fs::read_to_string(file) {
            Ok(c) => c,
            Err(e) => {
                warn!(file = %file.display(), error = %e, "skipping unreadable file");
                continue;
            }
        };
        // Already shimmed (e.g. a re-run) or no binding use: nothing to do.
        if content.contains(CONTEXT_SHIM) || !re.is_match(&content) {
            continue;
        }

        let label = "prepend `#let rheo-context = rheo-context()` compatibility shim";
        info!(file = %file.display(), rewrite = label, "shim context binding");
        println!("{}: {}", file.display(), label);

        if apply {
            let shimmed = format!("{CONTEXT_SHIM}\n{content}");
            fs::write(file, shimmed.as_bytes())
                .map_err(|e| RheoError::io(e, format!("writing {}", file.display())))?;
        }
    }

    Ok(())
}

/// Reports (never rewrites) every removed `[html] feed_*` rheo.toml key and
/// `#let rheo-*` binding still present, each with its location and a pointer
/// to its replacement. A feed's title/author/base-url/inclusion rules do not
/// map one-to-one onto `@rheo/feeds`'s Typst configuration, so a mechanical
/// rewrite would produce something subtly wrong — this stays report-only.
fn report_removed_feed_surface(project: &ProjectConfig) {
    if let Some(html) = project.config.plugin_sections.get("html") {
        for retired in RETIRED_KEYS.iter().filter(|r| r.table == "[html]") {
            if html.extra.contains_key(retired.key) {
                report_removed("rheo.toml [html]", retired.key, retired.replacement);
            }
        }
    }

    let content_dir = resolve_effective_content_dir(project);
    let typ_files = collect_typ_files(&content_dir);
    // Built from REMOVED_VAR_BINDINGS so the names live in one place: the table
    // below is already consulted for each match's replacement text.
    let names = REMOVED_VAR_BINDINGS
        .iter()
        .map(|(name, _)| regex::escape(name))
        .collect::<Vec<_>>()
        .join("|");
    let re = Regex::new(&format!(r"(?m)^\s*#let\s+({names})\b"))
        .expect("alternation over REMOVED_VAR_BINDINGS must compile");

    for file in &typ_files {
        let Ok(content) = fs::read_to_string(file) else {
            continue;
        };
        for caps in re.captures_iter(&content) {
            let name = &caps[1];
            let line = line_number(&content, caps.get(0).unwrap().start());
            let replacement = REMOVED_VAR_BINDINGS
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, r)| *r)
                .expect("matched name comes from REMOVED_VAR_BINDINGS");
            report_removed(&format!("{}:{}", file.display(), line), name, replacement);
        }
    }
}

/// One location's finding: `<location>: \`<name>\` — <replacement>`. The one
/// print format both the rheo.toml key scan and the `.typ` binding scan
/// above share.
fn report_removed(location: &str, name: &str, replacement: &str) {
    println!("{location}: `{name}` — {replacement}");
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
fn expand_vertebrae_patterns(patterns: &[String], content_dir: &Path) -> HashSet<PathBuf> {
    let mut matched = HashSet::new();
    for pattern in patterns {
        let glob_pattern = content_dir.join(pattern).display().to_string();
        let Ok(paths) = glob::glob(&glob_pattern) else {
            continue;
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
    config_path: &Path,
    apply: bool,
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

    let text = fs::read_to_string(config_path)
        .map_err(|e| RheoError::io(e, format!("reading {}", config_path.display())))?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| {
        RheoError::project_config(format!("failed to parse {}: {}", config_path.display(), e))
    })?;

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
            println!(
                "  - [{}]: vertebrae matched the full scan; removing (no exclude needed)",
                site.label()
            );
        } else {
            println!(
                "  - [{}]: vertebrae -> exclude = {:?}",
                site.label(),
                newly_included
            );
        }

        if apply {
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
    }

    if apply {
        fs::write(config_path, doc.to_string())
            .map_err(|e| RheoError::io(e, format!("writing {}", config_path.display())))?;
    }

    Ok(())
}

/// Resolve a `.typ` link href to an absolute path.
///
/// `/`-prefixed hrefs are resolved against the content directory (Typst's root);
/// relative hrefs (including `./`) are resolved against the linking file's
/// directory.
fn resolve_href(href: &str, file_dir: &Path, content_dir: &Path) -> Option<PathBuf> {
    let p = Path::new(href);
    let resolved = if p.is_absolute() {
        content_dir.join(p.strip_prefix("/").unwrap_or(p))
    } else {
        file_dir.join(p)
    };
    Some(resolved)
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

/// Rewrite the top-level `version` key in `rheo.toml`, preserving all other
/// formatting via `toml_edit` (a serde round-trip would drop comments and
/// reformat the file).
fn bump_version(config_path: &Path, target: &ManifestVersion) -> Result<()> {
    let text = fs::read_to_string(config_path)
        .map_err(|e| RheoError::io(e, format!("reading {}", config_path.display())))?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| {
        RheoError::project_config(format!("failed to parse {}: {}", config_path.display(), e))
    })?;

    doc["version"] = toml_edit::value(target.to_string());

    fs::write(config_path, doc.to_string())
        .map_err(|e| RheoError::io(e, format!("writing {}", config_path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_version_orders() {
        let old = ManifestVersion::parse("0.3.0").unwrap();
        let new = ManifestVersion::parse("0.4.0").unwrap();
        assert!(old < new);
        assert!(new > old);
    }

    #[test]
    fn bump_version_preserves_formatting() {
        let dir = tempfile::tempdir().unwrap();
        let toml_path = dir.path().join("rheo.toml");
        let original = "# a leading comment\nversion = \"0.3.0\"\ncontent_dir = \"pages\"\n\n[pdf.spine]\nvertebrae = [\"a.typ\"]\ntitle = \"Book\"\n";
        fs::write(&toml_path, original).unwrap();

        let target = ManifestVersion::parse("0.4.0").unwrap();
        bump_version(&toml_path, &target).unwrap();

        let updated = fs::read_to_string(&toml_path).unwrap();
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
        let rel = resolve_href("./sibling.typ", file_dir, content_dir).unwrap();
        assert_eq!(rel, PathBuf::from("/proj/content/ch1/sibling.typ"));

        // Rooted at the content directory.
        let root = resolve_href("/intro.typ", file_dir, content_dir).unwrap();
        assert_eq!(root, PathBuf::from("/proj/content/intro.typ"));
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
            mode: rheo_core::config::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        migrate_link_syntax(&project, true).unwrap();

        let rewritten = fs::read_to_string(content.join("intro.typ")).unwrap();
        assert!(rewritten.contains("#link(<about>)[About]"));
        // External URL untouched.
        assert!(rewritten.contains("#link(\"https://example.com\")[ex]"));
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

        let project = ProjectConfig {
            root: root.to_path_buf(),
            name: "test".into(),
            config: rheo_core::RheoConfig {
                version: ManifestVersion::parse("0.4.0").unwrap(),
                content_dir: Some("content".into()),
                ..Default::default()
            },
            typ_files: vec![content.join("page.typ")],
            mode: rheo_core::config::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.4.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        migrate_target_references(&project, true).unwrap();

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

        let project = ProjectConfig {
            root: root.to_path_buf(),
            name: "test".into(),
            config: rheo_core::RheoConfig {
                version: ManifestVersion::parse("0.5.0").unwrap(),
                content_dir: Some("content".into()),
                ..Default::default()
            },
            typ_files: vec![content.join("page.typ"), content.join("link_only.typ")],
            mode: rheo_core::config::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.5.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        migrate_context_references(&project, true).unwrap();

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
            mode: rheo_core::config::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        let toml_path = root.join("rheo.toml");
        fs::write(
            &toml_path,
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n\n[epub.spine]\ntitle = \"Book\"\nvertebrae = [\"main.typ\"]\n",
        )
        .unwrap();

        // Dry run: no write.
        migrate_vertebrae_to_exclude(&project, &toml_path, false).unwrap();
        let unchanged = fs::read_to_string(&toml_path).unwrap();
        assert!(unchanged.contains("vertebrae"));

        migrate_vertebrae_to_exclude(&project, &toml_path, true).unwrap();
        let updated = fs::read_to_string(&toml_path).unwrap();
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
            mode: rheo_core::config::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        let toml_path = root.join("rheo.toml");
        fs::write(
            &toml_path,
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n\n[pdf.spine]\nvertebrae = [\"*.typ\"]\n",
        )
        .unwrap();

        migrate_vertebrae_to_exclude(&project, &toml_path, true).unwrap();
        let updated = fs::read_to_string(&toml_path).unwrap();
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
            mode: rheo_core::config::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        migrate_link_syntax(&project, true).unwrap();

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
            mode: rheo_core::config::project::ProjectMode::Directory,
            config_path: Some(root.join("rheo.toml")),
        };
        fs::write(
            root.join("rheo.toml"),
            "version = \"0.3.0\"\ncontent_dir = \"content\"\n",
        )
        .unwrap();

        migrate_link_syntax(&project, true).unwrap();

        let rewritten = fs::read_to_string(content.join("intro.typ")).unwrap();
        // Nested collision member -> path-qualified primary handle `chapters:intro`,
        // never the ambiguous escape form `<intro.typ>`.
        assert!(rewritten.contains("#link(<chapters:intro>)[nested]"));
        assert!(!rewritten.contains("<intro.typ>"));
    }
}

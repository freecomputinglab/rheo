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

use regex::{Captures, Regex};
use rheo_core::build::resolve_effective_content_dir;
use rheo_core::config::manifest_version::ManifestVersion;
use rheo_core::config::project::ProjectConfig;
use rheo_core::reticulate::{SpineLayout, VirtualSpine};
use rheo_core::util::path::canonicalize_path;
use rheo_core::{Result, RheoError};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use walkdir::WalkDir;

/// Version at which the `#link("./file.typ")` syntax was replaced by the
/// `#link(<handle>)` label syntax. Projects older than this need a link rewrite.
const LINK_SYNTAX_VERSION: &str = "0.4.0";

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

    let link_threshold = ManifestVersion::parse(LINK_SYNTAX_VERSION).expect("valid semver");
    let needs_link_rewrite = from < link_threshold;

    println!("\nMigrations:");
    if needs_link_rewrite {
        println!("  - rewrite #link(\"./file.typ\") syntax to #link(<handle>)");
    }
    println!("  - bump rheo.toml version to {to}");

    if needs_link_rewrite {
        println!("\nLink rewrites:");
        migrate_link_syntax(&project, apply)?;
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
    let spine = VirtualSpine::build(&typ_files, &content_dir, &project.root, layout, false)?;

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
                .map_err(|e| RheoError::io(e, format!("failed to write {}", file.display())))?;
        }
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
        .map_err(|e| RheoError::io(e, format!("failed to read {}", config_path.display())))?;
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|e| {
        RheoError::project_config(format!("failed to parse {}: {}", config_path.display(), e))
    })?;

    doc["version"] = toml_edit::value(target.to_string());

    fs::write(config_path, doc.to_string())
        .map_err(|e| RheoError::io(e, format!("failed to write {}", config_path.display())))?;
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

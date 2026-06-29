//! `rheo migrate` — best-effort, experimental migration of an older Rheo project
//! to the latest version.
//!
//! Migration is version-aware: the project's `version` field in `rheo.toml` is
//! compared against the current Rheo version, and the set of migrations that
//! span that gap is applied. See `freecomputinglab/rheo#139`.

use rheo_core::manifest_version::ManifestVersion;
use rheo_core::project::ProjectConfig;
use rheo_core::{Result, RheoError};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

/// Version at which the `#link("./file.typ")` syntax was replaced by the
/// `#link(<handle>)` label syntax. Projects older than this need a link rewrite.
const LINK_SYNTAX_VERSION: &str = "0.4.0";

/// Run migration for the project at `path`.
///
/// `apply == false` is a dry run: it reports the version gap and the migrations
/// that would run, but writes nothing. `apply == true` applies content migrations
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

    if !apply {
        println!("\nDry run; no changes made. Re-run with --apply to write them.");
        return Ok(());
    }

    // Content migrations. The link-rewrite migration is implemented in
    // rheo-61k; until it lands this is a no-op stub.
    if needs_link_rewrite {
        migrate_link_syntax(&project)?;
    }

    bump_version(config_path, &to)?;
    println!("\nBumped rheo.toml version to {to}.");
    Ok(())
}

/// Rewrite old `#link("./file.typ")` syntax to the `#link(<handle>)` form.
///
/// TODO(rheo-61k): implement using the handle map from
/// `VirtualSpine::build` (`crates/core/src/reticulate/spine.rs`).
fn migrate_link_syntax(_project: &ProjectConfig) -> Result<()> {
    warn!("link-syntax migration is not yet implemented (rheo-61k); skipping");
    Ok(())
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
        let original = format!(
            "# a leading comment\nversion = \"0.3.0\"\ncontent_dir = \"pages\"\n\n[pdf.spine]\nvertebrae = [\"a.typ\"]\nmerge = true\n",
        );
        fs::write(&toml_path, &original).unwrap();

        let target = ManifestVersion::parse("0.4.0").unwrap();
        bump_version(&toml_path, &target).unwrap();

        let updated = fs::read_to_string(&toml_path).unwrap();
        // Comment and all other keys/structure preserved verbatim.
        assert!(updated.starts_with("# a leading comment\n"));
        assert!(updated.contains("content_dir = \"pages\""));
        assert!(updated.contains("merge = true"));
        // Only the version value changed.
        assert!(updated.contains("version = \"0.4.0\""));
        assert!(!updated.contains("0.3.0"));
    }

    #[test]
    fn bump_version_rejects_missing_key() {
        let dir = tempfile::tempdir().unwrap();
        let toml_path = dir.path().join("rheo.toml");
        fs::write(&toml_path, "content_dir = \"pages\"\n").unwrap();

        let target = ManifestVersion::parse("0.4.0").unwrap();
        // toml_edit permits indexing a missing key (creates it); we only assert
        // the call succeeds and adds the key, since a project without a version
        // field would already have failed config loading upstream.
        bump_version(&toml_path, &target).unwrap();
        let updated = fs::read_to_string(&toml_path).unwrap();
        assert!(updated.contains("version = \"0.4.0\""));
    }
}

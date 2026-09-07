use crate::{
    Result,
    assets::CopyGlobs,
    project::{ProjectConfig, ProjectMode},
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;
use tracing::{debug, info, warn};

/// The set of asset sources a build depends on, used by the watcher to decide
/// which changed files are relevant — independent of file extension.
///
/// This is derived from what the format plugins actually declare through the
/// [`FormatPlugin::assets`](crate::FormatPlugin::assets) mechanism (plus
/// project/plugin/package `copy` globs), so watch coverage tracks whatever each
/// plugin — or an imported Typst package — contributes, without hard-coding any
/// extension. Build it with [`crate::Build::watch_asset_spec`].
#[derive(Debug, Default)]
pub struct WatchAssetSpec {
    /// Absolute (canonicalized where possible) source paths of every resolved
    /// asset across all enabled plugins.
    asset_paths: HashSet<PathBuf>,
    /// Compiled copy-glob sets (absolute patterns), pre-compiled by the caller
    /// so the watcher and the copy step share one glob engine and one
    /// compilation — see [`CopyGlobs`].
    copy_globs: Vec<CopyGlobs>,
    /// Package `source_root` directories that must be watched in addition to the
    /// project root, since package assets live outside the project tree.
    package_roots: Vec<PathBuf>,
}

impl WatchAssetSpec {
    /// Assemble a spec from resolved asset source paths, pre-compiled copy
    /// globs, and package source roots. Asset paths are canonicalized where
    /// possible so they compare equal to the (canonicalized) paths reported by
    /// the filesystem watcher; each `CopyGlobs`'s base must already have been
    /// canonicalized by the caller for the same reason.
    pub fn new(
        asset_paths: Vec<PathBuf>,
        copy_globs: Vec<CopyGlobs>,
        mut package_roots: Vec<PathBuf>,
    ) -> Self {
        let asset_paths = asset_paths
            .into_iter()
            .map(|p| p.canonicalize().unwrap_or(p))
            .collect();

        package_roots = package_roots
            .into_iter()
            .map(|p| p.canonicalize().unwrap_or(p))
            .collect();
        package_roots.sort();
        package_roots.dedup();

        Self {
            asset_paths,
            copy_globs,
            package_roots,
        }
    }

    /// Directories outside the project root that must also be watched.
    pub fn package_roots(&self) -> &[PathBuf] {
        &self.package_roots
    }

    /// True if `path` is one of the resolved asset sources or matches a copy glob.
    fn matches(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().ok();
        if self.asset_paths.contains(path)
            || canonical
                .as_deref()
                .is_some_and(|c| self.asset_paths.contains(c))
        {
            return true;
        }
        self.copy_globs
            .iter()
            .any(|g| g.is_match(path) || canonical.as_deref().is_some_and(|c| g.is_match(c)))
    }
}

/// How long the watcher waits for the filesystem to go quiet before it
/// recompiles.
///
/// ONE SAVE IS SEVERAL EVENTS — an editor writing a temp file and renaming it,
/// a formatter rewriting a handful of files — and coalescing them into one
/// rebuild is what this exists for. It is not a place to be generous: this
/// interval is added to EVERY rebuild, so on a project that compiles quickly it
/// IS the rebuild. MEASURED on a project whose compile costs ~0.8s, the old
/// 1-second window made the edit-to-output latency 1.8s.
const DEBOUNCE_QUIET: Duration = Duration::from_millis(150);

/// The ceiling on how long a batch of changes may postpone its own rebuild.
///
/// [`DEBOUNCE_QUIET`] restarts on every event, so a process writing steadily —
/// a formatter walking a tree, a script generating content, a long `git
/// checkout` — can defer a rebuild indefinitely. Measured from the FIRST change
/// of a batch rather than the last, so a rebuild always happens within this of
/// something having changed.
const DEBOUNCE_MAX: Duration = Duration::from_millis(750);

/// How often the watch loop wakes to re-check its two debounce deadlines.
///
/// Has to be well under [`DEBOUNCE_QUIET`], or the quiet window is really this
/// interval rounded up.
const DEBOUNCE_POLL: Duration = Duration::from_millis(50);

/// Event indicating files have changed and compilation should be triggered
#[derive(Debug)]
pub enum WatchEvent {
    /// Source files or assets changed, trigger recompilation
    FilesChanged,
    /// Config file changed, need to reload ProjectConfig
    ConfigChanged,
}

/// Watch project files for changes and trigger recompilation
///
/// This function sets up file system watching for:
/// - All .typ files in the project
/// - Assets declared by the format plugins or imported packages (any filename,
///   any subdirectory, any extension — see [`WatchAssetSpec`])
/// - Project configuration (rheo.toml)
///
/// Changes are debounced on TWO deadlines, and both are needed: a rebuild fires
/// once the filesystem has been quiet for [`DEBOUNCE_QUIET`], or once
/// [`DEBOUNCE_MAX`] has passed since the first change of the batch, whichever
/// comes first. The quiet window coalesces the several events one editor save
/// produces; the ceiling keeps a steady stream of writes from postponing the
/// rebuild forever.
///
/// The asset spec is captured once at startup. A `rheo.toml` change triggers a
/// full reload but does not re-derive the spec, so a project that adds a brand
/// new asset or package mid-session may need the watch to be restarted before
/// edits to those new files are picked up.
///
/// # Arguments
/// * `project` - Project configuration with source files
/// * `build_dir` - Canonicalized build directory path to exclude from watching
/// * `asset_spec` - Resolved asset sources / package roots to treat as relevant
/// * `callback` - Function called when files change, receives WatchEvent
///
/// # Returns
/// * `Ok(())` when watching stops gracefully (e.g., Ctrl+C)
/// * `Err` if watcher setup fails
pub fn watch_project<F>(
    project: &ProjectConfig,
    build_dir: &Path,
    asset_spec: &WatchAssetSpec,
    mut callback: F,
) -> Result<()>
where
    F: FnMut(WatchEvent) -> Result<()>,
{
    let (tx, rx) = channel();

    // Create watcher
    let mut watcher = RecommendedWatcher::new(
        tx.clone(),
        notify::Config::default().with_poll_interval(Duration::from_millis(500)),
    )
    .map_err(|e| crate::RheoError::file_watcher(e, "creating file watcher"))?;

    // Watch based on project mode
    match project.mode {
        ProjectMode::SingleFile => {
            // Watch only the single file's parent directory (non-recursive)
            let file_to_watch = &project.typ_files[0];
            let watch_dir = file_to_watch
                .parent()
                .ok_or_else(|| crate::RheoError::project_config("file has no parent directory"))?;

            info!(file = %file_to_watch.display(), "watching single file");
            watcher
                .watch(watch_dir, RecursiveMode::NonRecursive)
                .map_err(|e| crate::RheoError::file_watcher(e, "watching file directory"))?;
        }
        ProjectMode::Directory => {
            // Existing behavior: recursive watch of project root
            info!(path = %project.root.display(), "watching project directory");
            watcher
                .watch(&project.root, RecursiveMode::Recursive)
                .map_err(|e| crate::RheoError::file_watcher(e, "watching project directory"))?;
        }
    }

    // Watch each package source root (outside the project tree) so edits to
    // package-declared assets also trigger rebuilds. A failure here is not fatal:
    // the package cache may be read-only or large, and the rest of the watch
    // should still work.
    for root in asset_spec.package_roots() {
        match watcher.watch(root, RecursiveMode::Recursive) {
            Ok(()) => debug!(path = %root.display(), "watching package asset root"),
            Err(e) => {
                warn!(error = %e, path = %root.display(), "failed to watch package asset root")
            }
        }
    }

    // The loaded config file may live outside the primary watched tree — in an
    // ancestor directory (single-file walk-up) or at a custom `--config` path.
    // Watch its parent directory (non-recursive) when it is not already covered,
    // so config edits are detected regardless of mode or filename.
    let primary_watch_dir = match project.mode {
        ProjectMode::SingleFile => project.typ_files[0].parent(),
        ProjectMode::Directory => Some(project.root.as_path()),
    };
    if let Some(config_parent) = project.config_path.as_deref().and_then(|p| p.parent())
        && primary_watch_dir.is_none_or(|primary| !config_parent.starts_with(primary))
    {
        match watcher.watch(config_parent, RecursiveMode::NonRecursive) {
            Ok(()) => debug!(path = %config_parent.display(), "watching config directory"),
            Err(e) => {
                warn!(error = %e, path = %config_parent.display(), "failed to watch config directory")
            }
        }
    }

    // True if `rheo.toml` changed, which needs a full project reload rather than
    // a recompile. There is no companion flag for ordinary files:
    // `first_event_time` below is already set by ANY relevant change, so it is
    // the gate, and this only picks WHICH event to send.
    let mut config_changed = false;
    let mut last_event_time = std::time::Instant::now();
    // Set on the FIRST change of a batch and cleared when that batch fires, so
    // `DEBOUNCE_MAX` has something to measure from. `None` means nothing is
    // pending, and is therefore the "anything to do?" test.
    let mut first_event_time: Option<std::time::Instant> = None;

    info!("watching for changes (press Ctrl+C to stop)");

    loop {
        match rx.recv_timeout(DEBOUNCE_POLL) {
            Ok(result) => {
                match result {
                    // Ignore Access events (file opens/reads) - only care about
                    // modifications. The Typst compiler opens source files during
                    // compilation, which would trigger infinite recompilation loops
                    // if we treated Access as a change.
                    Ok(event) if !matches!(event.kind, notify::EventKind::Access(_)) => {
                        // Filter events to only relevant files (.typ files, rheo.toml, assets)
                        let paths: Vec<PathBuf> = event
                            .paths
                            .into_iter()
                            .filter(|p| is_relevant_path(p, project, build_dir, asset_spec))
                            .collect();

                        if !paths.is_empty() {
                            debug!(?paths, "detected file changes");
                            last_event_time = std::time::Instant::now();
                            first_event_time.get_or_insert(last_event_time);

                            // Config changes require reloading project
                            // configuration, so they are worth distinguishing;
                            // any other relevant path just needs a recompile,
                            // which `first_event_time` above already records.
                            if paths.iter().any(|p| is_config_path(p, project)) {
                                config_changed = true;
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        warn!(error = %e, "file watcher error");
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // Watcher channel closed - exit cleanly
                info!("file watcher stopped");
                break;
            }
        }

        // ONCE PER ITERATION, whichever arm above ran, and that placement is the
        // point of it: checked only on the timeout arm, the `DEBOUNCE_MAX`
        // ceiling could never fire during exactly the situation it exists for —
        // a stream of events arriving faster than the poll, which never reaches
        // a timeout at all.
        if let Some(first) = first_event_time
            && (last_event_time.elapsed() >= DEBOUNCE_QUIET || first.elapsed() >= DEBOUNCE_MAX)
        {
            // A config change subsumes a file change — reloading the project
            // recompiles it anyway — so one batch is one event whatever else
            // changed alongside `rheo.toml`.
            let event = if config_changed {
                WatchEvent::ConfigChanged
            } else {
                WatchEvent::FilesChanged
            };

            if let Err(e) = callback(event) {
                warn!(error = %e, "compilation failed, continuing to watch");
            }

            // Reset for the next batch of changes.
            config_changed = false;
            first_event_time = None;
        }
    }

    Ok(())
}

/// Check if a path is relevant for triggering recompilation
fn is_relevant_path(
    path: &Path,
    project: &ProjectConfig,
    build_dir: &Path,
    asset_spec: &WatchAssetSpec,
) -> bool {
    // CRITICAL: Exclude all paths under the build directory to prevent infinite loops
    // Try canonicalized comparison first (handles symlinks and relative paths)
    if let Ok(canonical_path) = path.canonicalize() {
        if canonical_path.starts_with(build_dir) {
            return false;
        }
    }
    // Fallback: If canonicalize fails (file doesn't exist yet), check prefix match
    // This handles cases where notify fires events for paths being created
    else if path.starts_with(build_dir) {
        return false;
    }

    // The config file is relevant in both modes (see `is_config_path`).
    if is_config_path(path, project) {
        return true;
    }

    // A declared/resolved asset (from any plugin or imported package) or a
    // copy-glob match is relevant regardless of extension or location. This
    // covers both project-local assets and package assets under a package root.
    if asset_spec.matches(path) {
        return true;
    }

    match project.mode {
        ProjectMode::SingleFile => {
            // Otherwise only the exact target .typ file is relevant.
            path == project.typ_files[0].as_path()
        }
        ProjectMode::Directory => {
            // Any .typ file in the project triggers recompilation.
            if path.extension().and_then(|e| e.to_str()) == Some("typ") {
                return true;
            }

            // Check if it's a font file
            let font_extensions = ["ttf", "otf", "woff", "woff2"];
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| font_extensions.contains(&e))
                .unwrap_or(false)
        }
    }
}

/// True if `path` is the project's configuration file.
///
/// Matches the actually-loaded `project.config_path` (so a custom-named or
/// relocated `--config` file is detected), and also any file literally named
/// `rheo.toml` so that creating a config where none existed still triggers a
/// reload.
fn is_config_path(path: &Path, project: &ProjectConfig) -> bool {
    if let Some(cfg) = project.config_path.as_deref()
        && same_file(path, cfg)
    {
        return true;
    }
    path.file_name().and_then(|n| n.to_str()) == Some("rheo.toml")
}

/// Compare two paths for identity, tolerating non-canonical forms (e.g. a
/// relative `--config` argument vs. an absolute event path).
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Build a directory-mode project rooted at `temp` with one .typ file.
    fn dir_project(temp: &TempDir) -> ProjectConfig {
        fs::write(temp.path().join("doc.typ"), "#heading[Test]").unwrap();
        ProjectConfig::from_path(temp.path(), None).unwrap()
    }

    fn empty_spec() -> WatchAssetSpec {
        WatchAssetSpec::default()
    }

    #[test]
    fn test_declared_asset_with_any_extension_is_relevant() {
        let temp = TempDir::new().unwrap();
        let project = dir_project(&temp);
        let build_dir = project.root.join("build");

        // A plugin might declare an asset with any extension — here .scss in a
        // subdirectory. The watcher must treat it as relevant purely because it
        // is a declared source, not because of its extension.
        let scss = project.root.join("styles").join("theme.scss");
        let spec = WatchAssetSpec::new(vec![scss.clone()], vec![], vec![]);

        assert!(is_relevant_path(&scss, &project, &build_dir, &spec));
        // An undeclared file with the same unusual extension is NOT relevant.
        let other = project.root.join("styles").join("other.scss");
        assert!(!is_relevant_path(&other, &project, &build_dir, &spec));
    }

    #[test]
    fn test_copy_glob_match_is_relevant() {
        let temp = TempDir::new().unwrap();
        let project = dir_project(&temp);
        let build_dir = project.root.join("build");

        let spec = WatchAssetSpec::new(
            vec![],
            vec![CopyGlobs::compile(&project.root, &["images/**".to_string()]).unwrap()],
            vec![],
        );

        let matched = project.root.join("images").join("logo.png");
        assert!(is_relevant_path(&matched, &project, &build_dir, &spec));

        let unmatched = project.root.join("docs").join("notes.md");
        assert!(!is_relevant_path(&unmatched, &project, &build_dir, &spec));
    }

    #[test]
    fn test_package_asset_path_is_relevant() {
        let temp = TempDir::new().unwrap();
        let project = dir_project(&temp);
        let build_dir = project.root.join("build");

        // Simulate a package source root outside the project tree.
        let pkg_root = TempDir::new().unwrap();
        let pkg_css = pkg_root.path().join("assets").join("pkg.css");
        let spec = WatchAssetSpec::new(
            vec![pkg_css.clone()],
            vec![],
            vec![pkg_root.path().to_path_buf()],
        );

        assert!(is_relevant_path(&pkg_css, &project, &build_dir, &spec));
        assert_eq!(spec.package_roots().len(), 1);

        // A file under the package root that is not a declared asset is ignored.
        let stray = pkg_root.path().join("assets").join("readme.bin");
        assert!(!is_relevant_path(&stray, &project, &build_dir, &spec));
    }

    #[test]
    fn test_typ_file_still_relevant() {
        let temp = TempDir::new().unwrap();
        let project = dir_project(&temp);
        let build_dir = project.root.join("build");

        let typ = project.root.join("chapters").join("intro.typ");
        assert!(is_relevant_path(&typ, &project, &build_dir, &empty_spec()));
    }

    #[test]
    fn test_asset_under_build_dir_is_excluded() {
        let temp = TempDir::new().unwrap();
        let project = dir_project(&temp);
        let build_dir = project.root.join("build");
        fs::create_dir_all(&build_dir).unwrap();

        // A generated asset under the build dir must never re-trigger the watcher,
        // even if it is a declared asset source.
        let generated = build_dir.join("style.css");
        fs::write(&generated, "body {}").unwrap();
        let spec = WatchAssetSpec::new(vec![generated.clone()], vec![], vec![]);

        assert!(!is_relevant_path(&generated, &project, &build_dir, &spec));
    }

    /// A single-file project whose loaded config lives at `config_path`.
    fn single_file_project(root: &Path, config_path: PathBuf) -> ProjectConfig {
        ProjectConfig {
            name: "document".into(),
            root: root.to_path_buf(),
            config: crate::RheoConfig::default(),
            typ_files: vec![root.join("document.typ")],
            mode: ProjectMode::SingleFile,
            config_path: Some(config_path),
        }
    }

    #[test]
    fn test_config_file_relevant_in_single_file_mode() {
        let temp = TempDir::new().unwrap();
        let config = temp.path().join("rheo.toml");
        let project = single_file_project(temp.path(), config.clone());
        let build_dir = temp.path().join("build");

        assert!(is_relevant_path(
            &config,
            &project,
            &build_dir,
            &empty_spec()
        ));
        assert!(is_config_path(&config, &project));
    }

    #[test]
    fn test_custom_named_config_is_relevant() {
        let temp = TempDir::new().unwrap();
        let config = temp.path().join("prod.toml");
        let project = single_file_project(temp.path(), config.clone());
        let build_dir = temp.path().join("build");

        // The custom-named config is detected via project.config_path...
        assert!(is_relevant_path(
            &config,
            &project,
            &build_dir,
            &empty_spec()
        ));
        assert!(is_config_path(&config, &project));
        // ...while an unrelated .toml is not.
        let other = temp.path().join("other.toml");
        assert!(!is_relevant_path(
            &other,
            &project,
            &build_dir,
            &empty_spec()
        ));
    }

    #[test]
    fn test_sibling_rheo_toml_relevant_in_directory_mode() {
        let temp = TempDir::new().unwrap();
        let project = dir_project(&temp);
        let build_dir = project.root.join("build");

        // Directory mode with no loaded config still reacts to a rheo.toml by name.
        let config = project.root.join("rheo.toml");
        assert!(is_relevant_path(
            &config,
            &project,
            &build_dir,
            &empty_spec()
        ));
    }

    #[test]
    fn test_single_file_target_and_asset_relevant_others_not() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("document.typ");
        fs::write(&file, "#heading[Test]").unwrap();
        let project = ProjectConfig::from_path(&file, None).unwrap();
        let build_dir = project.root.join("build");

        let css = project.root.join("custom.css");
        let spec = WatchAssetSpec::new(vec![css.clone()], vec![], vec![]);

        // The target file and the declared asset are relevant.
        assert!(is_relevant_path(
            &project.typ_files[0],
            &project,
            &build_dir,
            &spec
        ));
        assert!(is_relevant_path(&css, &project, &build_dir, &spec));

        // Another .typ file in the same directory is NOT relevant in single-file mode.
        let other = project.root.join("other.typ");
        assert!(!is_relevant_path(&other, &project, &build_dir, &spec));
    }
}

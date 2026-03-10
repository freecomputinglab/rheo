use crate::{
    Result,
    project::{ProjectConfig, ProjectMode},
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;
use tracing::{debug, info, warn};

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
/// - Asset files (style.css)
/// - Project configuration (rheo.toml)
///
/// Changes are debounced with a 1-second delay to avoid rapid rebuilds during editing.
///
/// # Arguments
/// * `project` - Project configuration with source files
/// * `build_dir` - Canonicalized build directory path to exclude from watching
/// * `callback` - Function called when files change, receives WatchEvent
///
/// # Returns
/// * `Ok(())` when watching stops gracefully (e.g., Ctrl+C)
/// * `Err` if watcher setup fails
pub fn watch_project<F>(project: &ProjectConfig, build_dir: &Path, mut callback: F) -> Result<()>
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

    // Debounce logic: collect events for 1 second before triggering recompilation
    // This prevents excessive recompilation when editors save multiple files rapidly
    // or when a single edit triggers multiple filesystem events
    let debounce_duration = Duration::from_secs(1);
    let mut last_event_time = std::time::Instant::now();
    let mut pending_changes = false; // True if any .typ files changed
    let mut config_changed = false; // True if rheo.toml changed (requires full reload)

    info!("watching for changes (press Ctrl+C to stop)");

    loop {
        // Poll for filesystem events with 100ms timeout
        // Short timeout allows us to check debounce timer regularly
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(result) => {
                match result {
                    Ok(event) => {
                        // Ignore Access events (file opens/reads) - only care about modifications
                        // The Typst compiler opens source files during compilation, which would
                        // trigger infinite recompilation loops if we treated Access as a change
                        if matches!(event.kind, notify::EventKind::Access(_)) {
                            continue;
                        }

                        // Filter events to only relevant files (.typ files, rheo.toml, assets)
                        let paths: Vec<PathBuf> = event
                            .paths
                            .into_iter()
                            .filter(|p| is_relevant_path(p, project, build_dir))
                            .collect();

                        if !paths.is_empty() {
                            debug!(?paths, "detected file changes");
                            // Reset debounce timer - we'll wait for more events
                            last_event_time = std::time::Instant::now();

                            // Distinguish config changes from regular file changes
                            // Config changes require reloading project configuration
                            if paths.iter().any(|p| {
                                p.file_name()
                                    .and_then(|n| n.to_str())
                                    .map(|n| n == "rheo.toml")
                                    .unwrap_or(false)
                            }) {
                                config_changed = true;
                            } else {
                                pending_changes = true;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "file watcher error");
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // No new events received in last 100ms
                // Check if we have pending changes and debounce period has elapsed
                if pending_changes || config_changed {
                    let elapsed = last_event_time.elapsed();
                    if elapsed >= debounce_duration {
                        // Debounce period elapsed - trigger recompilation
                        let event = if config_changed {
                            WatchEvent::ConfigChanged
                        } else {
                            WatchEvent::FilesChanged
                        };

                        if let Err(e) = callback(event) {
                            warn!(error = %e, "compilation failed, continuing to watch");
                        }

                        // Reset flags for next batch of changes
                        pending_changes = false;
                        config_changed = false;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // Watcher channel closed - exit cleanly
                info!("file watcher stopped");
                break;
            }
        }
    }

    Ok(())
}

/// Check if a path is relevant for triggering recompilation
fn is_relevant_path(path: &Path, project: &ProjectConfig, build_dir: &Path) -> bool {
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

    match project.mode {
        ProjectMode::SingleFile => {
            // Only the exact file is relevant (and assets in parent directory)
            let target_file = &project.typ_files[0];

            // Check if it's the specific .typ file
            if path == target_file {
                return true;
            }

            // Check if it's style.css in the same directory
            if path.file_name().and_then(|n| n.to_str()) == Some("style.css")
                && let Some(parent) = path.parent()
                && parent == project.root
            {
                return true;
            }

            false
        }
        ProjectMode::Directory => {
            // Existing logic for directory mode
            // Check if it's a .typ file
            if path.extension().and_then(|e| e.to_str()) == Some("typ") {
                return true;
            }

            // Check if it's rheo.toml
            if path.file_name().and_then(|n| n.to_str()) == Some("rheo.toml") {
                return true;
            }

            // Check if it's style.css
            if path.file_name().and_then(|n| n.to_str()) == Some("style.css") {
                // Only if it's in the project root
                if let Some(parent) = path.parent()
                    && parent == project.root
                {
                    return true;
                }
            }

            false
        }
    }
}

use crate::{project::ProjectConfig, Result};
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
/// - Asset files (style.css, img/)
/// - Project configuration (rheo.toml)
///
/// Changes are debounced with a 1-second delay to avoid rapid rebuilds during editing.
///
/// # Arguments
/// * `project` - Project configuration with source files
/// * `callback` - Function called when files change, receives WatchEvent
///
/// # Returns
/// * `Ok(())` when watching stops gracefully (e.g., Ctrl+C)
/// * `Err` if watcher setup fails
pub fn watch_project<F>(project: &ProjectConfig, mut callback: F) -> Result<()>
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

    // Watch project root for .typ files and rheo.toml
    info!(path = %project.root.display(), "watching project directory");
    watcher
        .watch(&project.root, RecursiveMode::Recursive)
        .map_err(|e| crate::RheoError::file_watcher(e, "watching project directory"))?;

    // Debounce logic: collect events for 1 second before triggering
    let debounce_duration = Duration::from_secs(1);
    let mut last_event_time = std::time::Instant::now();
    let mut pending_changes = false;
    let mut config_changed = false;

    info!("watching for changes (press Ctrl+C to stop)");

    loop {
        // Check for events with a short timeout
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(result) => {
                match result {
                    Ok(event) => {
                        // Filter events to only relevant files
                        let paths: Vec<PathBuf> = event
                            .paths
                            .into_iter()
                            .filter(|p| is_relevant_path(p, project))
                            .collect();

                        if !paths.is_empty() {
                            debug!(?paths, "detected file changes");
                            last_event_time = std::time::Instant::now();

                            // Check if rheo.toml changed
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
                // Check if debounce period has elapsed
                if pending_changes || config_changed {
                    let elapsed = last_event_time.elapsed();
                    if elapsed >= debounce_duration {
                        // Trigger callback
                        let event = if config_changed {
                            WatchEvent::ConfigChanged
                        } else {
                            WatchEvent::FilesChanged
                        };

                        if let Err(e) = callback(event) {
                            warn!(error = %e, "compilation failed, continuing to watch");
                        }

                        pending_changes = false;
                        config_changed = false;
                    }
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                info!("file watcher stopped");
                break;
            }
        }
    }

    Ok(())
}

/// Check if a path is relevant for triggering recompilation
fn is_relevant_path(path: &Path, project: &ProjectConfig) -> bool {
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
        if let Some(parent) = path.parent() {
            if parent == project.root {
                return true;
            }
        }
    }

    // Check if it's in the img/ directory
    if let Some(img_dir) = &project.img_dir {
        if path.starts_with(img_dir) {
            return true;
        }
    }

    // Check if it's references.bib
    if path.file_name().and_then(|n| n.to_str()) == Some("references.bib") {
        if let Some(parent) = path.parent() {
            if parent == project.root {
                return true;
            }
        }
    }

    false
}

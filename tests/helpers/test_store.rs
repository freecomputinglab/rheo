use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Copies project source files to test store directory, excluding build artifacts.
///
/// For `rheo.toml` files missing a `version` field, automatically injects
/// `version = "<crate version>"` so test cases don't need to hardcode it.
pub fn copy_project_to_test_store(project_path: &Path, test_store: &Path) -> Result<(), String> {
    // Create test store
    fs::create_dir_all(test_store).map_err(|e| format!("Failed to create test store: {}", e))?;

    // Copy all project files except build/
    for entry in WalkDir::new(project_path) {
        let entry = entry.map_err(|e| format!("Walk error: {}", e))?;
        let rel_path = entry
            .path()
            .strip_prefix(project_path)
            .map_err(|e| format!("Path error: {}", e))?;

        // Skip build directory
        if rel_path.starts_with("build") {
            continue;
        }

        let dest = test_store.join(rel_path);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest).map_err(|e| format!("Dir creation error: {}", e))?;
        } else if entry.path().file_name().is_some_and(|n| n == "rheo.toml") {
            copy_rheo_toml_with_version(entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest).map_err(|e| format!("File copy error: {}", e))?;
        }
    }

    Ok(())
}

/// Copies a rheo.toml file, injecting the version field if missing.
fn copy_rheo_toml_with_version(src: &Path, dest: &Path) -> Result<(), String> {
    let content =
        fs::read_to_string(src).map_err(|e| format!("Failed to read {}: {}", src.display(), e))?;

    if content.contains("version =") || content.contains("version=") {
        // Already has a version field, copy as-is
        fs::write(dest, content)
            .map_err(|e| format!("Failed to write {}: {}", dest.display(), e))?;
    } else {
        // Inject version at the top
        let versioned = format!(
            "version = \"{}\"\n{}",
            env!("CARGO_PKG_VERSION"),
            content
        );
        fs::write(dest, versioned)
            .map_err(|e| format!("Failed to write {}: {}", dest.display(), e))?;
    }

    Ok(())
}

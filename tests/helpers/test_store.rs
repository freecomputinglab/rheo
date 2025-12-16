use std::fs;
use std::path::Path;
use walkdir::WalkDir;

/// Copies project source files to test store directory, excluding build artifacts
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
        } else {
            fs::copy(entry.path(), &dest).map_err(|e| format!("File copy error: {}", e))?;
        }
    }

    Ok(())
}

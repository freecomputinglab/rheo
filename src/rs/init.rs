use std::fs;
use std::path::Path;
use tracing::info;

use crate::Result;

const RHEO_TOML: &str = include_str!("../../examples/init_template/rheo.toml");
const STYLE_CSS: &str = include_str!("../../examples/init_template/style.css");
const INDEX_TYP: &str = include_str!("../../examples/init_template/content/index.typ");
const ABOUT_TYP: &str = include_str!("../../examples/init_template/content/about.typ");
const REFERENCES_BIB: &str = include_str!("../../examples/init_template/content/references.bib");
const HEADER_SVG: &str = include_str!("../../examples/init_template/content/img/header.svg");

/// Initialize a new Rheo project at the given path.
///
/// Creates the directory if it doesn't exist. Fails if the directory contains
/// any non-hidden entries (hidden files/dirs like `.git` or `.jj` are ignored).
pub fn init_project(path: &Path) -> Result<()> {
    // Create directory if it doesn't exist
    fs::create_dir_all(path)
        .map_err(|e| crate::RheoError::io(e, format!("creating directory {}", path.display())))?;

    // Check that directory is empty (ignoring hidden entries)
    let has_non_hidden = fs::read_dir(path)
        .map_err(|e| crate::RheoError::io(e, format!("reading directory {}", path.display())))?
        .filter_map(|entry| entry.ok())
        .any(|entry| {
            entry
                .file_name()
                .to_str()
                .is_none_or(|name| !name.starts_with('.'))
        });

    if has_non_hidden {
        return Err(crate::RheoError::project_config(format!(
            "directory {} is not empty",
            path.display()
        )));
    }

    // Create subdirectories
    fs::create_dir_all(path.join("content/img"))
        .map_err(|e| crate::RheoError::io(e, "creating content/img directory"))?;

    // Write template files
    let files: &[(&str, &str)] = &[
        ("rheo.toml", RHEO_TOML),
        ("style.css", STYLE_CSS),
        ("content/index.typ", INDEX_TYP),
        ("content/about.typ", ABOUT_TYP),
        ("content/references.bib", REFERENCES_BIB),
        ("content/img/header.svg", HEADER_SVG),
    ];

    for (rel_path, content) in files {
        let dest = path.join(rel_path);
        fs::write(&dest, content)
            .map_err(|e| crate::RheoError::io(e, format!("writing {}", rel_path)))?;
        info!(file = %rel_path, "created");
    }

    info!(path = %path.display(), "project initialized");
    Ok(())
}

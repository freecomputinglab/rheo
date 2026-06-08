use crate::pdf_utils::{DocumentTitle, sanitize_label_name};
use crate::plugins::SpineOptions;
use crate::reticulate::transformer::LinkTransformer;
use crate::reticulate::types::RheoValue;
use crate::{Result, RheoError, TYP_EXT};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A spine with relative linking transformations.
#[derive(Debug, Clone)]
pub struct BuiltSpine {
    /// The name of the file or website that the spine will generate.
    pub title: Option<String>,

    /// Whether or not the source has been merged into a single file.
    /// This is only true for PDF merged mode.
    pub is_merged: bool,

    /// Reticulated (relative link transformed) source files.
    /// Always length 1 if `is_merged`.
    pub source: Vec<String>,

    /// Validated `rheo-*` vars (prefix stripped) per vertebra, aligned with the
    /// original `spine_files` order — one map per original file even when
    /// `is_merged` collapses `source` to length 1.
    pub vars: Vec<HashMap<String, RheoValue>>,
}

impl BuiltSpine {
    /// Build a RheoSpine with AST-based link transformation for all output formats.
    ///
    /// # Arguments
    /// * `root` - Project root directory
    /// * `spine_config` - Optional spine configuration (determines spine files)
    /// * `format_ext` - The extension to use for relative links.
    /// * `merge_produces_pdf` - Whether the merged compile produces a PDF
    /// * `merge` - Whether to merge spine files into a single source (caller decides)
    pub fn build(
        root: &Path,
        spine_config: Option<&SpineOptions>,
        format_ext: &str,
        merge: bool,
    ) -> Result<BuiltSpine> {
        let spine_files = match spine_config {
            Some(spine) => spine.generate(root)?,
            None => collect_one_typst_file(root)?,
        };
        check_duplicate_filenames(&spine_files)?;

        // Merge when caller requests it (typically only PDF merged mode).
        // Other formats (epub, html) handle concatenation differently.

        let transformer = if format_ext == "pdf" && spine_files.len() > 1 {
            LinkTransformer::new(format_ext)
                .with_spine(spine_files.to_vec())
                .with_import_rewriting(merge)
        } else {
            LinkTransformer::new(format_ext).with_import_rewriting(merge)
        };

        let mut sources = Vec::new();
        let mut vars = Vec::new();

        for spine_file in &spine_files {
            let source = fs::read_to_string(spine_file).map_err(|e| {
                RheoError::project_config(format!(
                    "failed to read spine file '{}': {}",
                    spine_file.display(),
                    e
                ))
            })?;

            let output = transformer.transform_with_vars(&source, spine_file, root)?;
            let transformed_source = output.source;

            // A `None` value means the RHS was an unsupported kind. Only string
            // literals are supported for now, so report it as such.
            let mut file_vars = HashMap::new();
            for v in output.rheo_vars {
                match v.value {
                    Some(value) => {
                        file_vars.insert(v.key, value);
                    }
                    None => {
                        return Err(RheoError::invalid_data(format!(
                            "{}:{}: rheo-{} must be a string",
                            spine_file.display(),
                            v.line,
                            v.key
                        )));
                    }
                }
            }
            vars.push(file_vars);

            let final_source = if merge {
                let (label, doc_title) = extract_label_and_title(&source, spine_file)?;
                format!(
                    "#metadata(\"{}\") <{}>\n{}\n\n",
                    doc_title, label, transformed_source
                )
            } else {
                transformed_source
            };

            sources.push(final_source);
        }

        let final_sources = if merge {
            vec![sources.join("\n\n")]
        } else {
            sources
        };

        let title = spine_config.and_then(|s| s.title.clone());

        Ok(BuiltSpine {
            title,
            is_merged: merge,
            source: final_sources,
            vars,
        })
    }
}

fn extract_label_and_title(source: &str, spine_file: &Path) -> Result<(String, String)> {
    let filename = spine_file.file_name().ok_or_else(|| {
        RheoError::project_config(format!(
            "invalid filename in spine: '{}'",
            spine_file.display()
        ))
    })?;

    let filename_str = filename.to_string_lossy();
    let stem = filename_str.strip_suffix(TYP_EXT).unwrap_or(&filename_str);
    let label = sanitize_label_name(stem);
    let title = DocumentTitle::from_source(source, stem).extract();

    Ok((label, title))
}

fn check_duplicate_filenames(spine_files: &[PathBuf]) -> Result<()> {
    let mut seen: HashMap<String, &PathBuf> = HashMap::new();

    for spine_file in spine_files {
        if let Some(filename) = spine_file.file_name() {
            let key = filename.to_string_lossy().into_owned();
            match seen.entry(key) {
                Entry::Occupied(e) => {
                    return Err(RheoError::project_config(format!(
                        "duplicate filename in spine: '{}' appears at both '{}' and '{}'",
                        filename.to_string_lossy(),
                        e.get().display(),
                        spine_file.display()
                    )));
                }
                Entry::Vacant(e) => {
                    e.insert(spine_file);
                }
            }
        }
    }

    Ok(())
}

fn collect_typst_files(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| Some(entry.ok()?.path().to_path_buf()))
        .filter(|entry| {
            entry
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == &TYP_EXT[1..])
                .unwrap_or(false)
        })
        .collect()
}

fn collect_one_typst_file(root: &Path) -> Result<Vec<PathBuf>> {
    let typst_files = collect_typst_files(root);

    match typst_files.len() {
        0 => Err(RheoError::project_config("need at least one .typ file")),
        1 => Ok(typst_files),
        _ => Err(RheoError::project_config(
            "multiple .typ files found, specify spine configuration",
        )),
    }
}

fn collect_all_typst_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut typst_files = collect_typst_files(root);

    if typst_files.is_empty() {
        return Err(RheoError::project_config("need at least one .typ file"));
    }

    typst_files.sort();
    Ok(typst_files)
}

/// Generates a spine (ordered list of .typ files) based on configuration.
impl SpineOptions {
    /// Resolve vertebrae patterns into an ordered list of .typ files.
    ///
    /// If no vertebrae are configured, discovers all .typ files under `root`.
    pub fn generate(&self, root: &Path) -> Result<Vec<PathBuf>> {
        if self.vertebrae.is_empty() {
            return collect_all_typst_files(root);
        }

        let mut typst_files = Vec::new();
        for pattern in &self.vertebrae {
            let glob_pattern = root.join(pattern).display().to_string();
            let glob = glob::glob(&glob_pattern).map_err(|e| {
                RheoError::project_config(format!("invalid glob pattern '{}': {}", pattern, e))
            })?;

            let mut glob_files: Vec<PathBuf> = glob
                .filter_map(|entry| entry.ok())
                .filter(|path| path.is_file())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("typ"))
                .collect();

            // Sort by full path (lexicographic) for consistent ordering
            glob_files.sort();
            typst_files.extend(glob_files);
        }

        if typst_files.is_empty() {
            return Err(RheoError::project_config(
                "merge spine matched no .typ files",
            ));
        }

        Ok(typst_files)
    }
}

/// Generates a spine (ordered list of .typ files) based on configuration.
pub fn generate_spine(
    root: &Path,
    spine_config: Option<&SpineOptions>,
    require_spine: bool,
) -> Result<Vec<PathBuf>> {
    if require_spine && spine_config.is_none() {
        return Err(RheoError::project_config(
            "spine configuration required but not provided",
        ));
    }
    match spine_config {
        None => collect_one_typst_file(root),
        Some(spine) => spine.generate(root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir_with_files(files: &[&str]) -> TempDir {
        let temp = TempDir::new().unwrap();
        for file in files {
            let path = temp.path().join(file);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, "").unwrap();
        }
        temp
    }

    fn spine_with_vertebrae(vertebrae: Vec<String>) -> SpineOptions {
        SpineOptions {
            title: Some("Test".to_string()),
            vertebrae,
            merge: false,
        }
    }

    #[test]
    fn test_generate_spine_requires_merge_mode() {
        let temp = create_test_dir_with_files(&["test.typ"]);
        let result = generate_spine(temp.path(), None, true);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("spine configuration required")
        );
    }

    #[test]
    fn test_generate_spine_epub_single_file_fallback() {
        let temp = create_test_dir_with_files(&["single.typ"]);
        let result = generate_spine(temp.path(), None, false);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "single.typ");
    }

    #[test]
    fn test_generate_spine_epub_multiple_files_error() {
        let temp = create_test_dir_with_files(&["first.typ", "second.typ"]);
        let result = generate_spine(temp.path(), None, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("multiple .typ files found")
        );
    }

    #[test]
    fn test_generate_spine_epub_no_files_error() {
        let temp = create_test_dir_with_files(&["readme.md"]);
        let result = generate_spine(temp.path(), None, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("need at least one .typ file")
        );
    }

    #[test]
    fn test_generate_spine_with_vertebrae() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ", "c.typ"]);
        let spine = spine_with_vertebrae(vec!["*.typ".to_string()]);
        let result = generate_spine(temp.path(), Some(&spine), false);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_generate_spine_ordered_patterns() {
        let temp = create_test_dir_with_files(&[
            "cover.typ",
            "chapters/ch1.typ",
            "chapters/ch2.typ",
            "appendix.typ",
        ]);
        let spine = SpineOptions {
            title: Some("Book".to_string()),
            vertebrae: vec![
                "cover.typ".to_string(),
                "chapters/*.typ".to_string(),
                "appendix.typ".to_string(),
            ],
            merge: false,
        };
        let result = generate_spine(temp.path(), Some(&spine), true);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].file_name().unwrap(), "cover.typ");
        assert!(
            files[1]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("ch")
        );
        assert!(
            files[2]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("ch")
        );
        assert_eq!(files[3].file_name().unwrap(), "appendix.typ");
    }

    #[test]
    fn test_generate_spine_no_matches_error() {
        let temp = create_test_dir_with_files(&["readme.md"]);
        let spine = spine_with_vertebrae(vec!["*.typ".to_string()]);
        let result = generate_spine(temp.path(), Some(&spine), false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("merge spine matched no .typ files")
        );
    }

    #[test]
    fn test_generate_spine_empty_pattern_single_file() {
        let temp = create_test_dir_with_files(&["single.typ"]);
        let spine = spine_with_vertebrae(vec![]);
        let result = generate_spine(temp.path(), Some(&spine), false);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_generate_spine_empty_pattern_multiple_files_returns_all() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ"]);
        let spine = spine_with_vertebrae(vec![]);
        let result = generate_spine(temp.path(), Some(&spine), false);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 2);
    }

    fn write_spine_dir(files: &[(&str, &str)]) -> TempDir {
        let temp = TempDir::new().unwrap();
        for (name, contents) in files {
            fs::write(temp.path().join(name), contents).unwrap();
        }
        temp
    }

    #[test]
    fn test_build_collects_rheo_vars_per_file() {
        let temp = write_spine_dir(&[
            ("a.typ", "#let rheo-feed-title = \"Alpha\"\n= A"),
            ("b.typ", "= B, no vars"),
        ]);
        let spine = spine_with_vertebrae(vec!["*.typ".to_string()]);
        let built = BuiltSpine::build(temp.path(), Some(&spine), "html", false).unwrap();

        assert_eq!(built.vars.len(), 2);
        assert_eq!(
            built.vars[0].get("feed-title"),
            Some(&RheoValue::Str("Alpha".to_string()))
        );
        assert!(built.vars[1].is_empty());
    }

    #[test]
    fn test_build_errors_on_non_string_rheo_var() {
        let temp = write_spine_dir(&[("a.typ", "#let rheo-x = 1\n= A")]);
        let spine = spine_with_vertebrae(vec!["*.typ".to_string()]);
        let result = BuiltSpine::build(temp.path(), Some(&spine), "html", false);

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("a.typ"), "message missing path: {msg}");
        assert!(
            msg.contains("rheo-x must be a string"),
            "message missing reason: {msg}"
        );
    }
}

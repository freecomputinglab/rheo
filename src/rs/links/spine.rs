use super::types::RheoSpine;
use crate::config::Merge;
use crate::formats::pdf::{extract_document_title, sanitize_label_name};
use crate::{OutputFormat, Result, RheoError, TYP_EXT};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use typst::syntax::{FileId, Source, VirtualPath};
use walkdir::WalkDir;

/// Build a RheoSpine with AST-based link transformation for all output formats.
///
/// This unified function handles link transformation for PDF, HTML, and EPUB:
/// - PdfSingle: Removes .typ links, single source, no metadata heading
/// - PdfMerged: Converts .typ links to labels, injects metadata headings, merged into single source
/// - Html: Converts .typ links to .html, multiple sources (one per file), no metadata heading
/// - Epub: Converts .typ links to .xhtml, multiple sources (one per file), no metadata heading
///
/// # Arguments
/// * `root` - Project root directory
/// * `merge_config` - Optional merge configuration (determines spine files)
/// * `output_format` - Target output format (determines link transformation behavior)
/// * `title` - Document title (used for merged outputs)
///
/// # Returns
/// A RheoSpine containing transformed Typst sources ready for compilation.
pub fn build_rheo_spine(
    root: &Path,
    merge_config: Option<&Merge>,
    output_format: OutputFormat,
    title: &str,
) -> Result<RheoSpine> {
    // Generate spine: ordered list of .typ files
    let spine_files = generate_spine(root, merge_config, false)?;

    // Check for duplicate filenames
    check_duplicate_filenames(&spine_files)?;

    // Determine if we should merge sources based on format and config
    let should_merge = match output_format {
        OutputFormat::Pdf => merge_config.is_some(),
        OutputFormat::Html | OutputFormat::Epub => false,
    };

    let mut sources = Vec::new();

    for spine_file in &spine_files {
        // Read source content
        let source = fs::read_to_string(spine_file).map_err(|e| {
            RheoError::project_config(format!(
                "failed to read spine file '{}': {}",
                spine_file.display(),
                e
            ))
        })?;

        // Transform links using AST-based transformation
        let transformed_source =
            transform_source(&source, spine_file, &spine_files, output_format)?;

        // Add metadata heading only for merged PDF
        let final_source = if should_merge && output_format == OutputFormat::Pdf {
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

    // Merge sources if needed
    let final_sources = if should_merge {
        vec![sources.join("\n\n")]
    } else {
        sources
    };

    Ok(RheoSpine {
        title: title.to_string(),
        is_merged: should_merge,
        source: final_sources,
    })
}

/// Transform source using AST-based link transformation
fn transform_source(
    source: &str,
    spine_file: &Path,
    spine_files: &[PathBuf],
    output_format: OutputFormat,
) -> Result<String> {
    // Create a temporary file ID for AST parsing
    let temp_spine_file = if let Some(ext) = spine_file.extension() {
        let new_ext = format!("combined.{}", ext.to_string_lossy());
        spine_file.with_extension(new_ext)
    } else {
        spine_file.with_extension("combined")
    };

    let file_id = FileId::new(None, VirtualPath::new(temp_spine_file));
    let source_obj = Source::new(file_id, source.to_string());

    // Extract links and code blocks
    let links = crate::links::parser::extract_links(&source_obj);
    let code_ranges = crate::links::serializer::find_code_block_ranges(&source_obj);

    // Compute transformations based on format
    let spine_for_transform = match output_format {
        OutputFormat::Pdf => Some(spine_files),
        OutputFormat::Html | OutputFormat::Epub => None,
    };

    let transformations = crate::links::transformer::compute_transformations(
        &links,
        output_format,
        spine_for_transform,
        spine_file,
    )?;

    // Apply transformations
    Ok(crate::links::serializer::apply_transformations(
        source,
        &transformations,
        &code_ranges,
    ))
}

/// Extract label and title from source and filename
fn extract_label_and_title(source: &str, spine_file: &Path) -> Result<(String, String)> {
    let filename = spine_file.file_name().ok_or_else(|| {
        RheoError::project_config(format!("invalid filename in spine: '{}'", spine_file.display()))
    })?;

    let filename_str = filename.to_string_lossy();
    let stem = filename_str.strip_suffix(TYP_EXT).unwrap_or(&filename_str);
    let label = sanitize_label_name(stem);
    let title = extract_document_title(source, stem);

    Ok((label, title))
}

/// Check for duplicate filenames in spine
fn check_duplicate_filenames(spine_files: &[PathBuf]) -> Result<()> {
    let mut seen_filenames: HashSet<String> = HashSet::new();

    for spine_file in spine_files {
        if let Some(filename) = spine_file.file_name() {
            let filename_str = filename.to_string_lossy().to_string();

            if !seen_filenames.insert(filename_str.clone()) {
                // Find the first occurrence
                if let Some(first_occurrence) = spine_files.iter().find(|f| {
                    f.file_name()
                        .map(|n| n.to_string_lossy() == filename.to_string_lossy())
                        .unwrap_or(false)
                }) {
                    return Err(RheoError::project_config(format!(
                        "duplicate filename in spine: '{}' appears at both '{}' and '{}'",
                        filename_str,
                        first_occurrence.display(),
                        spine_file.display()
                    )));
                }
            }
        }
    }

    Ok(())
}

/// Concatenate multiple Typst source files into a single source for merged PDF compilation.
///
/// Each file in the spine is:
/// 1. Read from disk
/// 2. Title extracted from `#set document(title: [...])` or filename
/// 3. Prefixed with a level-1 heading containing the title and a label derived from filename
/// 4. Links to other .typ files transformed to label references
/// 5. Concatenated together
pub fn reticulate_typst_sources(spine_files: &[PathBuf], is_combined: bool) -> Result<RheoSpine> {
    // Check for duplicate filenames
    let mut seen_filenames: HashSet<String> = HashSet::new();
    let mut duplicate_paths: Vec<(String, PathBuf, PathBuf)> = Vec::new();

    for spine_file in spine_files {
        if let Some(filename) = spine_file.file_name() {
            let filename_str = filename.to_string_lossy().to_string();

            // Check if we've seen this filename before
            if !seen_filenames.insert(filename_str.clone()) {
                // Find the first occurrence
                if let Some(first_occurrence) = spine_files.iter().find(|f| {
                    f.file_name()
                        .map(|n| n.to_string_lossy() == filename.to_string_lossy())
                        .unwrap_or(false)
                }) {
                    duplicate_paths.push((
                        filename_str.clone(),
                        first_occurrence.clone(),
                        spine_file.clone(),
                    ));
                }
            }
        }
    }

    // Report first duplicate error if any
    if let Some((filename, first_path, second_path)) = duplicate_paths.first() {
        return Err(RheoError::project_config(format!(
            "duplicate filename in spine: '{}' appears at both '{}' and '{}'",
            filename,
            first_path.display(),
            second_path.display()
        )));
    }

    // Initialize source
    let mut spine_source: Vec<String> = vec![];

    for spine_file in spine_files {
        // Read source content
        let source = fs::read_to_string(spine_file).map_err(|e| {
            RheoError::project_config(format!(
                "failed to read spine file '{}': {}",
                spine_file.display(),
                e
            ))
        })?;

        // Derive label and title from filename (without extension)
        let (label, title) = if let Some(filename) = spine_file.file_name() {
            let filename_str = filename.to_string_lossy();
            let stem = filename_str.strip_suffix(TYP_EXT).unwrap_or(&filename_str);
            let label = sanitize_label_name(stem);
            let title = extract_document_title(&source, stem);
            (label, title)
        } else {
            return Err(RheoError::project_config(format!(
                "invalid filename in spine: '{}'",
                spine_file.display()
            )));
        };

        // Create temporary spine
        let temp_spine_file = if let Some(ext) = spine_file.extension() {
            // Create new extension: ".combined." + original extension
            let new_ext = format!("combined.{}", ext.to_string_lossy());
            spine_file.with_extension(new_ext)
        } else {
            // Handle case where there's no extension
            spine_file.with_extension("combined")
        };

        // Transform .typ links using AST-based transformation
        let file_id = FileId::new(None, VirtualPath::new(temp_spine_file));
        let source_obj = Source::new(file_id, source.clone());
        let links = crate::links::parser::extract_links(&source_obj);
        let code_ranges = crate::links::serializer::find_code_block_ranges(&source_obj);
        let transformations = crate::links::transformer::compute_transformations(
            &links,
            crate::OutputFormat::Pdf,
            Some(spine_files),
            spine_file,
        )?;
        let transformed_source = crate::links::serializer::apply_transformations(
            &source,
            &transformations,
            &code_ranges,
        );

        // Inject heading with label attached to metadata: #metadata(title) <label>
        spine_source.push(format!(
            "#metadata(\"{}\") <{}>{}\n\n",
            title, label, transformed_source
        ));
    }

    Ok(RheoSpine {
        title: "Untitled".to_string(), // TODO: derive from config or directory
        is_merged: is_combined,
        source: spine_source,
    })
}

fn collect_one_typst_file(root: &Path) -> Result<Vec<PathBuf>> {
    let typst_files: Vec<PathBuf> = WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| Some(entry.ok()?.path().to_path_buf()))
        .filter(|entry| {
            entry
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == &TYP_EXT[1..])
                .unwrap_or(false)
        })
        .collect();

    match typst_files.len() {
        0 => Err(RheoError::project_config("need at least one .typ file")),
        1 => Ok(typst_files),
        _ => Err(RheoError::project_config(
            "multiple .typ files found, specify spine in merge config",
        )),
    }
}

/// Generates a spine (ordered list of .typ files) based on configuration.
///
/// # Arguments
/// * `root` - Project root directory
/// * `merge_config` - Optional merge configuration with spine patterns
/// * `require_merge` - If true, merge_config must be provided (PDF mode)
///
/// # Errors
/// Returns error if:
/// - `require_merge=true` and `merge_config=None`
/// - No .typ files found (fallback mode)
/// - Multiple .typ files found without merge config (fallback mode)
/// - Merge spine matched no .typ files
pub fn generate_spine(
    root: &Path,
    merge_config: Option<&Merge>,
    require_merge: bool,
) -> Result<Vec<PathBuf>> {
    // PDF mode: merge config is required
    if require_merge && merge_config.is_none() {
        return Err(RheoError::project_config(
            "merge configuration required but not provided",
        ));
    }

    match merge_config {
        // Single-file mode
        None => collect_one_typst_file(root),

        // Empty spine pattern: auto-discover single file only
        // This is used for single-file mode with default EPUB merge config
        Some(merge) if merge.spine.is_empty() => collect_one_typst_file(root),

        // Spine is specified
        // Process glob patterns from merge config
        Some(merge) => {
            let mut typst_files = Vec::new();
            for pattern in &merge.spine {
                let glob_pattern = root.join(pattern).display().to_string();
                let glob = glob::glob(&glob_pattern).map_err(|e| {
                    RheoError::project_config(format!("invalid glob pattern '{}': {}", pattern, e))
                })?;

                let mut glob_files: Vec<PathBuf> = glob
                    .filter_map(|entry| entry.ok())
                    .filter(|path| path.is_file())
                    .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("typ"))
                    .filter(|path| path.file_name().is_some()) // Ensure path has a filename
                    .collect();

                // Sort lexicographically within each pattern
                glob_files.sort_by_cached_key(|p| {
                    p.file_name()
                        .expect("file_name() checked in filter above")
                        .to_os_string()
                });
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

    #[test]
    fn test_generate_spine_requires_merge_mode() {
        let temp = create_test_dir_with_files(&["test.typ"]);
        let result = generate_spine(temp.path(), None, true);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("merge configuration required")
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
    fn test_generate_spine_with_merge_config() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ", "c.typ"]);
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec!["*.typ".to_string()],
        };
        let result = generate_spine(temp.path(), Some(&merge), false);
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
        let merge = Merge {
            title: "Book".to_string(),
            spine: vec![
                "cover.typ".to_string(),
                "chapters/*.typ".to_string(),
                "appendix.typ".to_string(),
            ],
        };
        let result = generate_spine(temp.path(), Some(&merge), true);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 4);
        // Verify pattern order is preserved
        assert_eq!(files[0].file_name().unwrap(), "cover.typ");
        // ch1.typ and ch2.typ should be sorted lexicographically within their pattern
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
    fn test_generate_spine_merge_no_matches_error() {
        let temp = create_test_dir_with_files(&["readme.md"]);
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec!["*.typ".to_string()],
        };
        let result = generate_spine(temp.path(), Some(&merge), false);
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
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec![], // Empty spine
        };

        let result = generate_spine(temp.path(), Some(&merge), false);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "single.typ");
    }

    #[test]
    fn test_generate_spine_empty_pattern_multiple_files_error() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ"]);
        let merge = Merge {
            title: "Test".to_string(),
            spine: vec![], // Empty spine with multiple files
        };

        let result = generate_spine(temp.path(), Some(&merge), false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("multiple .typ files")
        );
    }

    #[test]
    fn test_fallback_lexicographic_ordering() {
        let temp = create_test_dir_with_files(&["single.typ"]);

        // Test that fallback with single file works and is ordered
        let result = generate_spine(temp.path(), None, false);
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_name().unwrap(), "single.typ");
    }
}

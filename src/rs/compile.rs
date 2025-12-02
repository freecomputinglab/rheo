///! Shared compilation utilities.
///!
///! Contains utilities used across multiple output formats,
///! such as remove_relative_typ_links().

use crate::config::PdfConfig;
use crate::formats::{html, pdf};
use crate::spine::generate_spine;
use crate::world::RheoWorld;
use crate::{OutputFormat, Result, RheoError};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use tracing::{debug, error, info, instrument, warn};
use typst::layout::PagedDocument;
use typst_pdf::PdfOptions;

/// Compile a single file to a specific format.
///
/// Note: EPUB is not supported (requires EpubConfig).
pub fn compile_format(
    format: OutputFormat,
    input: &Path,
    output: &Path,
    root: &Path,
    repo_root: &Path,
) -> Result<()> {
    match format {
        OutputFormat::Pdf => pdf::compile_pdf(input, output, root, repo_root),
        OutputFormat::Html => html::compile_html(input, output, root, repo_root),
        OutputFormat::Epub => Err(RheoError::project_config(
            "EPUB requires config, use formats::epub::compile_epub()",
        )),
    }
}

/// Remove relative .typ links from Typst source code for PDF/EPUB compilation.
///
/// For PDF and EPUB outputs, relative links to other .typ files don't make sense
/// (yet - in the future they may become document anchors for multi-file PDFs).
/// This function removes those links while preserving the link text.
///
/// # Arguments
/// * `source` - The Typst source code
///
/// # Returns
/// * `String` - Source code with relative .typ links removed
///
/// # Examples
/// ```
/// # use rheo::compile::remove_relative_typ_links;
/// let source = r#"See #link("./other.typ")[the other page] for details."#;
/// let result = remove_relative_typ_links(source);
/// assert_eq!(result, r#"See [the other page] for details."#);
/// ```
///
/// # Note
/// External URLs (http://, https://, etc.) are preserved unchanged.
///
/// # TODO
/// When multi-file PDF compilation is implemented, relative links should
/// become document anchors instead of being removed.
#[instrument(skip(source))]
pub fn remove_relative_typ_links(source: &str) -> String {
    // Regex to match Typst link function calls
    // Captures: #link("url")[body] or #link("url", body)
    // We need to handle:
    // 1. #link("./file.typ")[text] -> [text]
    // 2. #link("../dir/file.typ")[text] -> [text]
    // 3. #link("/abs/path.typ")[text] -> [text]
    // 4. #link("https://example.com")[text] -> preserve

    let re =
        Regex::new(r#"#link\("([^"]+)"\)(\[[^\]]+\]|,\s*[^)]+)"#).expect("invalid regex pattern");

    let result = re.replace_all(source, |caps: &regex::Captures| {
        let url = &caps[1];
        let body = &caps[2];

        // Check if this is a relative .typ link
        let is_relative_typ = url.ends_with(".typ")
            && !url.starts_with("http://")
            && !url.starts_with("https://")
            && !url.starts_with("mailto:");

        if is_relative_typ {
            // Remove the link, keep just the body
            if body.starts_with('[') {
                // #link("url")[body] -> [body]
                body.to_string()
            } else {
                // #link("url", body) -> body (without comma)
                body.trim_start_matches(',').trim().to_string()
            }
        } else {
            // Preserve the full link for external URLs
            format!("#link(\"{}\"){}", url, body)
        }
    });

    result.to_string()
}

/// Sanitize a filename to create a valid Typst label name.
///
/// Replaces non-alphanumeric characters (except hyphens and underscores) with underscores.
///
/// # Arguments
/// * `name` - The filename to sanitize
///
/// # Returns
/// * `String` - Sanitized label name
///
/// # Examples
/// ```
/// # use rheo::compile::sanitize_label_name;
/// assert_eq!(sanitize_label_name("chapter 01"), "chapter_01");
/// assert_eq!(sanitize_label_name("severance-01"), "severance-01");
/// assert_eq!(sanitize_label_name("my_file!@#"), "my_file___");
/// ```
pub fn sanitize_label_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Convert filename to readable title.
///
/// Transforms a filename stem into a human-readable title by replacing
/// separators with spaces and capitalizing words.
///
/// # Arguments
/// * `filename` - The filename stem (without .typ extension)
///
/// # Returns
/// * `String` - Title-cased readable title
///
/// # Examples
/// ```
/// # use rheo::compile::filename_to_title;
/// assert_eq!(filename_to_title("severance-ep-1"), "Severance Ep 1");
/// assert_eq!(filename_to_title("my_document"), "My Document");
/// ```
pub fn filename_to_title(filename: &str) -> String {
    filename
        .replace('-', " ")
        .replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Strip basic Typst markup to get plain text.
///
/// Removes common Typst markup patterns like #emph[...], #strong[...],
/// and italic markers (_) to extract plain text from formatted content.
///
/// # Arguments
/// * `text` - Text with Typst markup
///
/// # Returns
/// * `String` - Plain text with markup removed
fn strip_typst_markup(text: &str) -> String {
    // Remove #emph[...], #strong[...], etc.
    let re = Regex::new(r"#\w+\[([^\]]+)\]").expect("invalid regex");
    let result = re.replace_all(text, "$1");

    // Remove underscores (italic markers)
    let result = result.replace('_', "");

    result.trim().to_string()
}

/// Extract title from Typst document source.
///
/// Searches for `#set document(title: [...])` and extracts the content.
/// Falls back to filename if no title is found. The extracted title is
/// cleaned of basic Typst markup.
///
/// # Arguments
/// * `source` - The Typst source code
/// * `filename` - Filename to use as fallback (without .typ extension)
///
/// # Returns
/// * `String` - Extracted or fallback title
///
/// # Examples
/// ```
/// # use rheo::compile::extract_document_title;
/// let source = r#"#set document(title: [My Title])"#;
/// let title = extract_document_title(source, "fallback");
/// assert_eq!(title, "My Title");
/// ```
pub fn extract_document_title(source: &str, filename: &str) -> String {
    // Find the start of the title parameter
    if let Some(title_start) = source.find("#set document(") {
        let after_doc = &source[title_start..];
        if let Some(title_pos) = after_doc.find("title:") {
            let after_title = &after_doc[title_pos + 6..]; // Skip "title:"

            // Find the opening bracket for the title
            if let Some(bracket_start) = after_title.find('[') {
                let title_content = &after_title[bracket_start + 1..];

                // Count brackets to find the matching closing bracket
                let mut depth = 1;
                let mut end_pos = 0;

                for (i, ch) in title_content.chars().enumerate() {
                    if ch == '[' {
                        depth += 1;
                    } else if ch == ']' {
                        depth -= 1;
                        if depth == 0 {
                            end_pos = i;
                            break;
                        }
                    }
                }

                if end_pos > 0 {
                    let title = &title_content[..end_pos];
                    // Strip Typst markup for plain text
                    let cleaned = strip_typst_markup(title);
                    if !cleaned.trim().is_empty() {
                        return cleaned;
                    }
                }
            }
        }
    }

    // Fallback: use filename, convert to title case
    filename_to_title(filename)
}

/// Transform relative .typ links to label references for merged PDF compilation.
///
/// For merged PDF outputs, links to other .typ files should reference the label
/// at the start of each document section. This function transforms relative .typ
/// links to label references using the document's filename.
///
/// # Arguments
/// * `source` - The Typst source code
/// * `spine_files` - List of files in the spine (for validation)
/// * `current_file` - Path to the current file being processed
///
/// # Returns
/// * `Result<String>` - Source code with .typ links transformed to labels
///
/// # Examples
/// ```no_run
/// # use rheo::compile::transform_typ_links_to_labels;
/// # use std::path::PathBuf;
/// let source = r#"See #link("./chapter2.typ")[next chapter]"#;
/// let spine = vec![PathBuf::from("chapter1.typ"), PathBuf::from("chapter2.typ")];
/// let current = PathBuf::from("chapter1.typ");
/// let result = transform_typ_links_to_labels(source, &spine, &current).unwrap();
/// assert_eq!(result, r#"See #link(<chapter2>)[next chapter]"#);
/// ```
///
/// # Errors
/// Returns an error if a .typ link references a file not in the spine.
#[instrument(skip(source, spine_files))]
pub fn transform_typ_links_to_labels(
    source: &str,
    spine_files: &[PathBuf],
    current_file: &Path,
) -> Result<String> {
    // Build map: filename (without extension) -> sanitized label name
    let mut label_map: HashMap<String, String> = HashMap::new();
    for spine_file in spine_files {
        if let Some(filename) = spine_file.file_name() {
            let filename_str = filename.to_string_lossy();
            // Remove .typ extension
            let stem = filename_str.strip_suffix(".typ").unwrap_or(&filename_str);
            let label = sanitize_label_name(stem);
            label_map.insert(stem.to_string(), label);
        }
    }

    // Regex to match Typst link function calls
    // Captures: #link("url")[body] or #link("url", body)
    let re =
        Regex::new(r#"#link\("([^"]+)"\)(\[[^\]]+\]|,\s*[^)]+)"#).expect("invalid regex pattern");

    let mut errors = Vec::new();
    let result = re.replace_all(source, |caps: &regex::Captures| {
        let url = &caps[1];
        let body = &caps[2];

        // Check if this is a .typ link
        let is_typ_link = url.ends_with(".typ");

        // Check if it's an external URL or fragment-only link
        let is_external = url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("mailto:");
        let is_fragment = url.starts_with('#');

        if is_typ_link && !is_external {
            // Extract the filename from the path
            let path = Path::new(url);
            let filename = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(url);

            // Remove .typ extension for lookup
            let stem = filename.strip_suffix(".typ").unwrap_or(filename);

            // Look up in spine files map
            if let Some(label) = label_map.get(stem) {
                // Transform to label reference: #link(<label>)[...]
                format!("#link(<{}>){}", label, body)
            } else {
                // File not in spine - collect error
                errors.push(format!(
                    "Link target '{}' not found in spine. Add it to the spine in rheo.toml or remove the link.",
                    filename
                ));
                // Keep original link unchanged in output
                format!("#link(\"{}\"){}", url, body)
            }
        } else if is_fragment {
            // Preserve fragment-only links unchanged
            format!("#link(\"{}\"){}", url, body)
        } else {
            // Preserve external URLs unchanged
            format!("#link(\"{}\"){}", url, body)
        }
    });

    // If we collected any errors, return the first one
    if let Some(error_msg) = errors.first() {
        return Err(RheoError::project_config(error_msg));
    }

    Ok(result.to_string())
}

/// Concatenate multiple Typst source files into a single source for merged PDF compilation.
///
/// Each file in the spine is:
/// 1. Read from disk
/// 2. Title extracted from `#set document(title: [...])` or filename
/// 3. Prefixed with a level-1 heading containing the title and a label derived from filename
/// 4. Links to other .typ files transformed to label references
/// 5. Concatenated together
///
/// # Arguments
/// * `spine_files` - Ordered list of .typ files to concatenate
///
/// # Returns
/// * `Result<String>` - Concatenated source code with headings, labels, and transformed links
///
/// # Errors
/// * Returns an error if duplicate filenames are found (would create duplicate labels)
/// * Returns an error if link transformation fails
/// * Returns an error if file reading fails
///
/// # Format
/// ```typst
/// = Document Title <filename-label>
///
/// [original content with transformed links]
///
/// = Next Document <next-label>
///
/// [next document content]
/// ```
///
/// # Examples
/// ```no_run
/// # use rheo::compile::concatenate_typst_sources;
/// # use std::path::PathBuf;
/// let spine = vec![
///     PathBuf::from("chapter1.typ"),
///     PathBuf::from("chapter2.typ"),
/// ];
/// let result = concatenate_typst_sources(&spine).unwrap();
/// // Result will be:
/// // = Chapter 1 <chapter1>
/// //
/// // [contents of chapter1.typ with transformed links]
/// //
/// // = Chapter 2 <chapter2>
/// //
/// // [contents of chapter2.typ with transformed links]
/// ```
#[instrument(skip(spine_files))]
pub fn concatenate_typst_sources(spine_files: &[PathBuf]) -> Result<String> {
    // Check for duplicate filenames
    let mut seen_filenames: HashSet<String> = HashSet::new();
    let mut duplicate_paths: Vec<(String, PathBuf, PathBuf)> = Vec::new();

    for spine_file in spine_files {
        if let Some(filename) = spine_file.file_name() {
            let filename_str = filename.to_string_lossy().to_string();

            // Check if we've seen this filename before
            if !seen_filenames.insert(filename_str.clone()) {
                // Find the first occurrence
                if let Some(first_occurrence) = spine_files
                    .iter()
                    .find(|f| f.file_name().map(|n| n.to_string_lossy() == filename.to_string_lossy()).unwrap_or(false))
                {
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
        return Err(RheoError::project_config(&format!(
            "duplicate filename in spine: '{}' appears at both '{}' and '{}'",
            filename,
            first_path.display(),
            second_path.display()
        )));
    }

    // Concatenate all sources
    let mut concatenated = String::new();

    for spine_file in spine_files {
        // Read source content
        let source = fs::read_to_string(spine_file).map_err(|e| {
            RheoError::project_config(&format!(
                "failed to read spine file '{}': {}",
                spine_file.display(),
                e
            ))
        })?;

        // Derive label and title from filename (without extension)
        let (label, title) = if let Some(filename) = spine_file.file_name() {
            let filename_str = filename.to_string_lossy();
            let stem = filename_str.strip_suffix(".typ").unwrap_or(&filename_str);
            let label = sanitize_label_name(stem);
            let title = extract_document_title(&source, stem);
            (label, title)
        } else {
            return Err(RheoError::project_config(&format!(
                "invalid filename in spine: '{}'",
                spine_file.display()
            )));
        };

        // Transform .typ links to labels
        let transformed_source = transform_typ_links_to_labels(&source, spine_files, spine_file)?;

        // Inject heading with label at start: = Title <label>
        concatenated.push_str(&format!("= {} <{}>\n\n{}\n\n", title, label, transformed_source));
    }

    Ok(concatenated)
}

/// Compile multiple Typst files into a single merged PDF.
///
/// Generates a spine from the PDF merge configuration, concatenates all sources
/// with labels and transformed links, then compiles to a single PDF document.
///
/// # Arguments
/// * `config` - PDF configuration including merge settings
/// * `output_path` - Where to write the merged PDF
/// * `root` - Project root directory
/// * `repo_root` - Repository root for shared resources
///
/// # Returns
/// * `Result<()>` - Success or compilation error
///
/// # Errors
/// * Returns error if spine generation fails
/// * Returns error if source concatenation fails (duplicate filenames, link validation)
/// * Returns error if compilation fails
/// * Returns error if PDF export fails
#[instrument(skip_all, fields(output = %output_path.display()))]
pub fn compile_pdf_merged(
    config: &PdfConfig,
    output_path: &Path,
    root: &Path,
    repo_root: &Path,
) -> Result<()> {
    // Generate spine: ordered list of .typ files
    let spine = generate_spine(root, config.merge.as_ref(), true)?;
    debug!(file_count = spine.len(), "generated PDF spine");

    // Concatenate sources with labels and transformed links
    let concatenated_source = concatenate_typst_sources(&spine)?;
    debug!(
        source_length = concatenated_source.len(),
        "concatenated sources"
    );

    // Create temporary file with concatenated source in the root directory
    // (Typst compiler requires main file to be within root for imports)
    let mut temp_file = NamedTempFile::new_in(root)
        .map_err(|e| RheoError::io(e, "creating temporary file for merged PDF"))?;
    temp_file
        .write_all(concatenated_source.as_bytes())
        .map_err(|e| RheoError::io(e, "writing concatenated source to temporary file"))?;
    temp_file
        .flush()
        .map_err(|e| RheoError::io(e, "flushing temporary file"))?;

    let temp_path = temp_file.path();
    debug!(temp_path = %temp_path.display(), "created temporary file");

    // Create RheoWorld with temp file as main
    // remove_typ_links=false because links already transformed to labels
    let world = RheoWorld::new(root, temp_path, repo_root, false)?;

    // Compile to PagedDocument
    info!(output = %output_path.display(), "compiling merged PDF");
    let result = typst::compile::<PagedDocument>(&world);

    // Print warnings
    for warning in &result.warnings {
        warn!(message = %warning.message, "compilation warning");
    }

    // Get the document or return errors
    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for err in &errors {
                error!(message = %err.message, "compilation error");
            }
            let error_messages: Vec<String> =
                errors.iter().map(|e| e.message.to_string()).collect();
            return Err(RheoError::Compilation {
                count: errors.len(),
                errors: error_messages.join("\n"),
            });
        }
    };

    // Export PDF bytes
    // Note: PDF title is set via document metadata in Typst source, not PdfOptions
    debug!(output = %output_path.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default()).map_err(|errors| {
        for err in &errors {
            error!(message = %err.message, "PDF export error");
        }
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::PdfGeneration {
            count: errors.len(),
            errors: error_messages.join("\n"),
        }
    })?;

    // Write to output file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output_path, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output_path)))?;

    info!(output = %output_path.display(), "successfully compiled merged PDF");
    Ok(())
}

/// Compile multiple Typst files into a single merged PDF (incremental mode).
///
/// Same as compile_pdf_merged() but reuses an existing RheoWorld for faster
/// recompilation in watch mode.
///
/// # Arguments
/// * `world` - Existing RheoWorld to reuse
/// * `config` - PDF configuration including merge settings
/// * `output_path` - Where to write the merged PDF
/// * `root` - Project root directory
///
/// # Returns
/// * `Result<()>` - Success or compilation error
#[instrument(skip(world), fields(output = %output_path.display()))]
pub fn compile_pdf_merged_incremental(
    world: &mut RheoWorld,
    config: &PdfConfig,
    output_path: &Path,
    root: &Path,
) -> Result<()> {
    // Generate spine: ordered list of .typ files
    let spine = generate_spine(root, config.merge.as_ref(), true)?;
    debug!(file_count = spine.len(), "generated PDF spine");

    // Concatenate sources with labels and transformed links
    let concatenated_source = concatenate_typst_sources(&spine)?;
    debug!(
        source_length = concatenated_source.len(),
        "concatenated sources"
    );

    // Create temporary file with concatenated source in the root directory
    // (Typst compiler requires main file to be within root)
    let mut temp_file = NamedTempFile::new_in(root)
        .map_err(|e| RheoError::io(e, "creating temporary file for merged PDF"))?;
    temp_file
        .write_all(concatenated_source.as_bytes())
        .map_err(|e| RheoError::io(e, "writing concatenated source to temporary file"))?;
    temp_file
        .flush()
        .map_err(|e| RheoError::io(e, "flushing temporary file"))?;

    let temp_path = temp_file.path();
    debug!(temp_path = %temp_path.display(), "created temporary file");

    // Set main file in existing world
    world.set_main(temp_path)?;

    // Compile to PagedDocument
    info!("compiling merged PDF");
    let result = typst::compile::<PagedDocument>(world);

    // Print warnings
    for warning in &result.warnings {
        warn!(message = %warning.message, "compilation warning");
    }

    // Get the document or return errors
    let document = match result.output {
        Ok(doc) => doc,
        Err(errors) => {
            for err in &errors {
                error!(message = %err.message, "compilation error");
            }
            let error_messages: Vec<String> =
                errors.iter().map(|e| e.message.to_string()).collect();
            return Err(RheoError::Compilation {
                count: errors.len(),
                errors: error_messages.join("\n"),
            });
        }
    };

    // Export PDF bytes
    // Note: PDF title is set via document metadata in Typst source, not PdfOptions
    debug!(output = %output_path.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default()).map_err(|errors| {
        for err in &errors {
            error!(message = %err.message, "PDF export error");
        }
        let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::PdfGeneration {
            count: errors.len(),
            errors: error_messages.join("\n"),
        }
    })?;

    // Write to output file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output_path, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output_path)))?;

    info!(output = %output_path.display(), "successfully compiled merged PDF");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove_relative_typ_links_basic() {
        let source = r#"See #link("./other.typ")[the other page] for details."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"See [the other page] for details."#);
    }

    #[test]
    fn test_remove_relative_typ_links_parent_dir() {
        let source = r#"Check #link("../parent/file.typ")[parent file]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"Check [parent file]."#);
    }

    #[test]
    fn test_remove_relative_typ_links_absolute() {
        let source = r#"See #link("/absolute/path.typ")[absolute]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"See [absolute]."#);
    }

    #[test]
    fn test_remove_relative_typ_links_preserves_external() {
        let source = r#"Visit #link("https://example.com")[our website] or #link("mailto:test@example.com")[email us]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, source); // Should be unchanged
    }

    #[test]
    fn test_remove_relative_typ_links_mixed() {
        let source =
            r#"See #link("./local.typ")[local] and #link("https://example.com")[external]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(
            result,
            r#"See [local] and #link("https://example.com")[external]."#
        );
    }

    #[test]
    fn test_remove_relative_typ_links_multiple() {
        let source = r#"#link("./one.typ")[First], #link("./two.typ")[Second], and #link("./three.typ")[Third]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"[First], [Second], and [Third]."#);
    }

    #[test]
    fn test_remove_relative_typ_links_preserves_non_typ() {
        let source = r#"Download #link("./file.pdf")[the PDF] here."#;
        let result = remove_relative_typ_links(source);
        // .pdf files should be preserved since they're not .typ files
        assert_eq!(result, source);
    }

    #[test]
    fn test_sanitize_label_name() {
        assert_eq!(sanitize_label_name("chapter 01.typ"), "chapter_01_typ");
        assert_eq!(sanitize_label_name("chapter 01"), "chapter_01");
        assert_eq!(sanitize_label_name("severance-01.typ"), "severance-01_typ");
        assert_eq!(sanitize_label_name("severance-01"), "severance-01");
        assert_eq!(sanitize_label_name("my_file!@#.typ"), "my_file____typ");
        assert_eq!(sanitize_label_name("my_file!@#"), "my_file___");
    }

    #[test]
    fn test_transform_typ_links_basic() {
        let source = r#"See #link("./chapter2.typ")[next chapter]."#;
        let spine = vec![
            PathBuf::from("chapter1.typ"),
            PathBuf::from("chapter2.typ"),
        ];
        let current = PathBuf::from("chapter1.typ");
        let result = transform_typ_links_to_labels(source, &spine, &current).unwrap();
        assert_eq!(result, r#"See #link(<chapter2>)[next chapter]."#);
    }

    #[test]
    fn test_transform_typ_links_relative_paths() {
        let source = r#"See #link("../intro.typ")[intro] and #link("./chapter2.typ")[next]."#;
        let spine = vec![
            PathBuf::from("intro.typ"),
            PathBuf::from("chapter1.typ"),
            PathBuf::from("chapter2.typ"),
        ];
        let current = PathBuf::from("chapter1.typ");
        let result = transform_typ_links_to_labels(source, &spine, &current).unwrap();
        assert_eq!(
            result,
            r#"See #link(<intro>)[intro] and #link(<chapter2>)[next]."#
        );
    }

    #[test]
    fn test_transform_typ_links_not_in_spine() {
        let source = r#"See #link("./missing.typ")[missing]."#;
        let spine = vec![PathBuf::from("chapter1.typ")];
        let current = PathBuf::from("chapter1.typ");
        let result = transform_typ_links_to_labels(source, &spine, &current);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found in spine"));
    }

    #[test]
    fn test_transform_typ_links_preserves_external() {
        let source = r#"Visit #link("https://example.com")[our website] or #link("mailto:test@example.com")[email us]."#;
        let spine = vec![PathBuf::from("chapter1.typ")];
        let current = PathBuf::from("chapter1.typ");
        let result = transform_typ_links_to_labels(source, &spine, &current).unwrap();
        assert_eq!(result, source); // Should be unchanged
    }

    #[test]
    fn test_transform_typ_links_preserves_fragments() {
        let source = r##"See #link("#heading")[section]."##;
        let spine = vec![PathBuf::from("chapter1.typ")];
        let current = PathBuf::from("chapter1.typ");
        let result = transform_typ_links_to_labels(source, &spine, &current).unwrap();
        assert_eq!(result, source); // Should be unchanged
    }

    #[test]
    fn test_concatenate_typst_sources_basic() {
        use std::io::Write;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();

        // Create temporary files with test content
        let path1 = dir.path().join("chapter1.typ");
        let mut file1 = std::fs::File::create(&path1).unwrap();
        write!(file1, "= Chapter 1\nThis is chapter one.").unwrap();

        let path2 = dir.path().join("chapter2.typ");
        let mut file2 = std::fs::File::create(&path2).unwrap();
        write!(file2, "= Chapter 2\nThis is chapter two.").unwrap();

        let spine = vec![path1, path2];
        let result = concatenate_typst_sources(&spine).unwrap();

        // Check that heading-based labels are injected (derived from filename)
        // These should appear at the start of each section
        assert!(result.contains("<chapter1>"));
        assert!(result.contains("<chapter2>"));

        // Check for generated headings with labels
        assert!(result.contains("= Chapter1 <chapter1>") || result.contains("= Chapter 1 <chapter1>"));
        assert!(result.contains("= Chapter2 <chapter2>") || result.contains("= Chapter 2 <chapter2>"));

        // Check that content is preserved
        assert!(result.contains("This is chapter one."));
        assert!(result.contains("This is chapter two."));
    }

    #[test]
    fn test_concatenate_typst_sources_label_injection() {
        use std::io::Write;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();
        let path = dir.path().join("test-file.typ");
        let mut file = std::fs::File::create(&path).unwrap();
        write!(file, "Content here").unwrap();

        let spine = vec![path];
        let result = concatenate_typst_sources(&spine).unwrap();

        // Heading with label should be injected (title derived from filename)
        assert!(result.starts_with("= Test File <test-file>"));
    }

    #[test]
    fn test_concatenate_typst_sources_duplicate_filenames() {
        use std::io::Write;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();

        // Create two files with same name in different directories
        let dir1 = dir.path().join("dir1");
        std::fs::create_dir_all(&dir1).unwrap();
        let path1 = dir1.join("chapter.typ");
        let mut file1 = std::fs::File::create(&path1).unwrap();
        write!(file1, "Content 1").unwrap();

        let dir2 = dir.path().join("dir2");
        std::fs::create_dir_all(&dir2).unwrap();
        let path2 = dir2.join("chapter.typ");
        let mut file2 = std::fs::File::create(&path2).unwrap();
        write!(file2, "Content 2").unwrap();

        let spine = vec![path1, path2];
        let result = concatenate_typst_sources(&spine);

        // Should fail with duplicate filename error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("duplicate filename in spine"));
    }

    #[test]
    fn test_concatenate_typst_sources_link_transformation() {
        use std::io::Write;
        use tempfile::tempdir;

        let dir = tempdir().unwrap();

        let path1 = dir.path().join("chapter1.typ");
        let mut file1 = std::fs::File::create(&path1).unwrap();
        write!(file1, r#"See #link("./chapter2.typ")[next chapter]"#).unwrap();

        let path2 = dir.path().join("chapter2.typ");
        let mut file2 = std::fs::File::create(&path2).unwrap();
        write!(file2, "= Chapter 2").unwrap();

        let spine = vec![path1, path2];
        let result = concatenate_typst_sources(&spine).unwrap();

        // Link should be transformed to label
        assert!(result.contains("#link(<chapter2>)[next chapter]"));
    }

    #[test]
    fn test_filename_to_title() {
        assert_eq!(filename_to_title("severance-ep-1"), "Severance Ep 1");
        assert_eq!(filename_to_title("my_document"), "My Document");
        assert_eq!(filename_to_title("chapter-01"), "Chapter 01");
        assert_eq!(filename_to_title("hello_world"), "Hello World");
        assert_eq!(filename_to_title("single"), "Single");
    }

    #[test]
    fn test_strip_typst_markup_basic() {
        assert_eq!(strip_typst_markup("#emph[italic]"), "italic");
        assert_eq!(strip_typst_markup("#strong[bold]"), "bold");
        assert_eq!(strip_typst_markup("plain text"), "plain text");
    }

    #[test]
    fn test_strip_typst_markup_underscores() {
        assert_eq!(strip_typst_markup("_underscored_"), "underscored");
        assert_eq!(strip_typst_markup("some_text"), "sometext");
    }

    #[test]
    fn test_strip_typst_markup_combined() {
        assert_eq!(
            strip_typst_markup("#emph[italic] and _underscored_"),
            "italic and underscored"
        );
    }

    #[test]
    fn test_extract_document_title_from_metadata() {
        let source = r#"#set document(title: [My Great Title])

= Chapter 1
Content here."#;

        let title = extract_document_title(source, "fallback");
        assert_eq!(title, "My Great Title");
    }

    #[test]
    fn test_extract_document_title_fallback() {
        let source = r#"= Chapter 1
Content here."#;

        let title = extract_document_title(source, "my-chapter");
        assert_eq!(title, "My Chapter");
    }

    #[test]
    fn test_extract_document_title_with_markup() {
        let source = r#"#set document(title: [Good news about hell - #emph[Severance]])"#;

        let title = extract_document_title(source, "fallback");
        // Should strip #emph and underscores
        // Note: complex nested bracket handling is limited by regex
        assert!(title.contains("Good news"));
        assert!(title.contains("Severance"));
    }

    #[test]
    fn test_extract_document_title_empty() {
        let source = r#"#set document(title: [])

Content"#;

        let title = extract_document_title(source, "default-name");
        // Empty title should fall back to filename
        assert_eq!(title, "Default Name");
    }

    #[test]
    fn test_extract_document_title_complex() {
        let source =
            r#"#set document(title: [Half Loop - _Severance_ [s1/e2]], author: [Test])"#;

        let title = extract_document_title(source, "fallback");
        // Should extract title and strip markup
        assert!(title.contains("Half Loop"));
        assert!(title.contains("Severance"));
    }
}

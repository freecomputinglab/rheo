use crate::compile::RheoCompileOptions;
use crate::config::PdfConfig;
use crate::formats::common::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::spine::generate_spine;
use crate::world::RheoWorld;
use crate::{Result, RheoError};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use tracing::{debug, info, instrument};
use typst::layout::PagedDocument;
use typst_pdf::PdfOptions;

// ============================================================================
// Single-file PDF compilation (implementation functions)
// ============================================================================

/// Implementation: Compile a single Typst document to PDF (fresh compilation)
///
/// Uses the typst library with:
/// - Root set to content_dir or project root (for local file imports across directories)
/// - Shared resources available via repo_root in src/typst/ (rheo.typ)
#[instrument(skip_all, fields(input = %input.display(), output = %output.display()))]
fn compile_pdf_single_impl_fresh(
    input: &Path,
    output: &Path,
    root: &Path,
    repo_root: &Path,
) -> Result<()> {
    // Create the compilation world
    // For standalone PDF compilation, remove relative .typ links from source
    let world = RheoWorld::new(root, input, repo_root, true)?;

    // Compile the document
    info!(input = %input.display(), "compiling to PDF");
    let result = typst::compile::<PagedDocument>(&world);
    let document = unwrap_compilation_result(result, None::<fn(&_) -> bool>)?;

    // Export to PDF
    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))?;

    // Write to file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
}

/// Implementation: Compile a single Typst document to PDF (incremental compilation)
fn compile_pdf_single_impl(world: &RheoWorld, output: &Path) -> Result<()> {
    // Compile the document
    info!("compiling to PDF");
    let result = typst::compile::<PagedDocument>(world);
    let document = unwrap_compilation_result(result, None::<fn(&_) -> bool>)?;

    // Export to PDF
    debug!(output = %output.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))?;

    // Write to file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to PDF");
    Ok(())
}

// ============================================================================
// Helper functions for merged PDF compilation
// ============================================================================

/// Sanitize a filename to create a valid Typst label name.
///
/// Replaces non-alphanumeric characters (except hyphens and underscores) with underscores.
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
pub fn filename_to_title(filename: &str) -> String {
    filename
        .replace(['-', '_'], " ")
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
#[instrument(skip(source, spine_files))]
pub fn transform_typ_links_to_labels(
    source: &str,
    spine_files: &[PathBuf],
    _current_file: &Path,
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

    // Concatenate all sources
    let mut concatenated = String::new();

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
            let stem = filename_str.strip_suffix(".typ").unwrap_or(&filename_str);
            let label = sanitize_label_name(stem);
            let title = extract_document_title(&source, stem);
            (label, title)
        } else {
            return Err(RheoError::project_config(format!(
                "invalid filename in spine: '{}'",
                spine_file.display()
            )));
        };

        // Transform .typ links to labels
        let transformed_source = transform_typ_links_to_labels(&source, spine_files, spine_file)?;

        // Inject heading with label at start: = Title <label>
        concatenated.push_str(&format!(
            "= {} <{}>\n\n{}\n\n",
            title, label, transformed_source
        ));
    }

    Ok(concatenated)
}

// ============================================================================
// Merged PDF compilation (implementation functions)
// ============================================================================

/// Implementation: Compile multiple Typst files into a single merged PDF (fresh compilation)
///
/// Generates a spine from the PDF merge configuration, concatenates all sources
/// with labels and transformed links, then compiles to a single PDF document.
#[instrument(skip_all, fields(output = %output_path.display()))]
fn compile_pdf_merged_impl_fresh(
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
    let document = unwrap_compilation_result(result, None::<fn(&_) -> bool>)?;

    // Export PDF bytes
    // Note: PDF title is set via document metadata in Typst source, not PdfOptions
    debug!(output = %output_path.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))?;

    // Write to output file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output_path, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output_path)))?;

    info!(output = %output_path.display(), "successfully compiled merged PDF");
    Ok(())
}

/// Implementation: Compile multiple Typst files into a single merged PDF (incremental compilation)
///
/// Same as compile_pdf_merged_impl_fresh() but reuses an existing RheoWorld for faster
/// recompilation in watch mode.
#[instrument(skip(world), fields(output = %output_path.display()))]
fn compile_pdf_merged_impl(
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
    let document = unwrap_compilation_result(result, None::<fn(&_) -> bool>)?;

    // Export PDF bytes
    // Note: PDF title is set via document metadata in Typst source, not PdfOptions
    debug!(output = %output_path.display(), "exporting to PDF");
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
        .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))?;

    // Write to output file
    debug!(size = pdf_bytes.len(), "writing PDF file");
    std::fs::write(output_path, &pdf_bytes)
        .map_err(|e| RheoError::io(e, format!("writing PDF file to {:?}", output_path)))?;

    info!(output = %output_path.display(), "successfully compiled merged PDF");
    Ok(())
}

// ============================================================================
// Unified public API
// ============================================================================

/// Compile Typst document(s) to PDF.
///
/// Routes to the appropriate implementation based on options:
/// - Single file, fresh: compile_pdf_single_impl_fresh()
/// - Single file, incremental: compile_pdf_single_impl()
/// - Merged PDF, fresh: compile_pdf_merged_impl_fresh()
/// - Merged PDF, incremental: compile_pdf_merged_impl()
///
/// # Arguments
/// * `options` - Compilation options (input, output, root, repo_root, world)
/// * `pdf_config` - Optional PDF merge configuration (None for single-file)
///
/// # Returns
/// * `Result<()>` - Success or compilation error
pub fn compile_pdf_new(options: RheoCompileOptions, pdf_config: Option<&PdfConfig>) -> Result<()> {
    // Check if this is merged PDF compilation
    let is_merged = pdf_config.and_then(|c| c.merge.as_ref()).is_some();

    match (is_merged, options.world) {
        // Merged PDF, incremental
        (true, Some(world)) => {
            let config = pdf_config.ok_or_else(|| {
                RheoError::project_config("PDF config required for merged compilation")
            })?;
            compile_pdf_merged_impl(world, config, &options.output, &options.root)
        }
        // Merged PDF, fresh
        (true, None) => {
            let config = pdf_config.ok_or_else(|| {
                RheoError::project_config("PDF config required for merged compilation")
            })?;
            compile_pdf_merged_impl_fresh(
                config,
                &options.output,
                &options.root,
                &options.repo_root,
            )
        }
        // Single file, incremental
        (false, Some(world)) => compile_pdf_single_impl(world, &options.output),
        // Single file, fresh
        (false, None) => compile_pdf_single_impl_fresh(
            &options.input,
            &options.output,
            &options.root,
            &options.repo_root,
        ),
    }
}

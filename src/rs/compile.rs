use crate::world::RheoWorld;
use regex::Regex;
use std::path::PathBuf;

/// Common compilation options used across all output formats.
///
/// This struct encapsulates the core parameters needed for any compilation:
/// - Input file (the .typ file to compile)
/// - Output file (where to write the result)
/// - Root directory (for resolving imports)
/// - Repository root (for rheo.typ template)
/// - Optional RheoWorld (for incremental compilation)
pub struct RheoCompileOptions<'a> {
    /// The input .typ file to compile
    pub input: PathBuf,
    /// The output file path
    pub output: PathBuf,
    /// Root directory for resolving imports
    pub root: PathBuf,
    /// Repository root for rheo.typ
    pub repo_root: PathBuf,
    /// Optional existing RheoWorld for incremental compilation
    pub world: Option<&'a mut RheoWorld>,
}

impl<'a> RheoCompileOptions<'a> {
    /// Create compilation options for a fresh (non-incremental) compilation.
    ///
    /// # Arguments
    /// * `input` - The input .typ file to compile
    /// * `output` - The output file path
    /// * `root` - Root directory for resolving imports
    /// * `repo_root` - Repository root for rheo.typ
    pub fn new(
        input: impl Into<PathBuf>,
        output: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        repo_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            input: input.into(),
            output: output.into(),
            root: root.into(),
            repo_root: repo_root.into(),
            world: None,
        }
    }

    /// Create compilation options for incremental compilation.
    ///
    /// Reuses an existing RheoWorld for faster recompilation.
    ///
    /// # Arguments
    /// * `input` - The input .typ file to compile
    /// * `output` - The output file path
    /// * `root` - Root directory for resolving imports
    /// * `repo_root` - Repository root for rheo.typ
    /// * `world` - Mutable reference to existing RheoWorld
    pub fn incremental(
        input: impl Into<PathBuf>,
        output: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        repo_root: impl Into<PathBuf>,
        world: &'a mut RheoWorld,
    ) -> Self {
        Self {
            input: input.into(),
            output: output.into(),
            root: root.into(),
            repo_root: repo_root.into(),
            world: Some(world),
        }
    }

    /// Create compilation options for merged PDF compilation.
    ///
    /// Note: For merged compilation, the input file is typically a temporary
    /// file containing concatenated sources.
    ///
    /// # Arguments
    /// * `temp_input` - Temporary file with concatenated sources
    /// * `output` - The output PDF path
    /// * `root` - Project root directory
    /// * `repo_root` - Repository root for rheo.typ
    pub fn merged(
        temp_input: impl Into<PathBuf>,
        output: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        repo_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            input: temp_input.into(),
            output: output.into(),
            root: root.into(),
            repo_root: repo_root.into(),
            world: None,
        }
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
/// Links inside code blocks (backtick-delimited) are preserved unchanged.
///
/// # TODO
/// When multi-file PDF compilation is implemented, relative links should
/// become document anchors instead of being removed.
pub fn remove_relative_typ_links(source: &str) -> String {
    // Find all backtick-delimited code regions (both inline ` and block ```)
    let code_ranges = find_backtick_ranges(source);

    // Apply regex transformation only outside code blocks
    let re =
        Regex::new(r#"#link\("([^"]+)"\)(\[[^\]]+\]|,\s*[^)]+)"#).expect("invalid regex pattern");

    let mut result = String::new();
    let mut last_pos = 0;

    for mat in re.find_iter(source) {
        let match_start = mat.start();
        let match_end = mat.end();

        // Check if this match is inside a backtick-delimited code region
        let in_code_block = code_ranges
            .iter()
            .any(|(start, end)| match_start >= *start && match_end <= *end);

        // Add text before this match
        result.push_str(&source[last_pos..match_start]);

        if in_code_block {
            // Preserve the link as-is if it's in a code block
            result.push_str(mat.as_str());
        } else {
            // Transform the link if it's outside code blocks
            let caps = re.captures(mat.as_str()).unwrap();
            let url = &caps[1];
            let body = &caps[2];

            let is_relative_typ = url.ends_with(".typ")
                && !url.starts_with("http://")
                && !url.starts_with("https://")
                && !url.starts_with("mailto:");

            if is_relative_typ {
                // Remove the link, keep just the body
                if body.starts_with('[') {
                    result.push_str(body);
                } else {
                    result.push_str(body.trim_start_matches(',').trim());
                }
            } else {
                // Preserve external links
                result.push_str(mat.as_str());
            }
        }

        last_pos = match_end;
    }

    // Add remaining text
    result.push_str(&source[last_pos..]);
    result
}

/// Find all backtick-delimited code ranges in the source
fn find_backtick_ranges(source: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '`' {
            // Count consecutive backticks
            let mut backtick_count = 1;
            while i + backtick_count < chars.len() && chars[i + backtick_count] == '`' {
                backtick_count += 1;
            }

            // Find matching closing backticks
            let start = i;
            let mut j = i + backtick_count;
            let mut found_end = false;

            while j < chars.len() {
                if chars[j] == '`' {
                    // Count closing backticks
                    let mut closing_count = 1;
                    while j + closing_count < chars.len() && chars[j + closing_count] == '`' {
                        closing_count += 1;
                    }

                    if closing_count == backtick_count {
                        // Found matching closing backticks
                        let end = j + backtick_count;
                        ranges.push((start, end));
                        i = end;
                        found_end = true;
                        break;
                    }
                    j += closing_count;
                } else {
                    j += 1;
                }
            }

            if !found_end {
                i += backtick_count;
            }
        } else {
            i += 1;
        }
    }

    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::pdf;

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
    fn test_syntax_aware_basic() {
        // Verify function works with basic case
        let source = r#"See #link("./other.typ")[the other page] for details."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"See [the other page] for details."#);
    }

    #[test]
    fn test_syntax_aware_preserves_code_blocks() {
        // Links in code blocks (backtick-fenced) remain unchanged
        let source = r#"Example: `#link("./other.typ")[text]` is preserved."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, source); // Should be unchanged
    }

    #[test]
    fn test_syntax_aware_preserves_inline_code() {
        // Links in inline backticks remain unchanged
        let source = r#"Use `#link("./file.typ")[link]` in your code."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, source); // Should be unchanged
    }

    #[test]
    fn test_syntax_aware_transforms_real_links() {
        // Links outside code blocks are transformed
        let source = r#"See #link("./intro.typ")[intro] and #link("./chapter2.typ")[chapter 2]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"See [intro] and [chapter 2]."#);
    }

    #[test]
    fn test_syntax_aware_mixed_context() {
        // Combination of code blocks, inline code, and real links
        let source = r#"Real #link("./file.typ")[link] and code `#link("./code.typ")[example]`."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(
            result,
            r#"Real [link] and code `#link("./code.typ")[example]`."#
        );
    }

    #[test]
    fn test_syntax_aware_preserves_external() {
        // External links are preserved both in and out of code blocks
        let source =
            r#"Visit #link("https://example.com")[site] or `#link("https://api.com")[API]`."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, source); // Should be unchanged
    }

    #[test]
    fn test_syntax_aware_multiple_typ_links() {
        // Multiple .typ links should all be transformed
        let source = r#"#link("./one.typ")[First], #link("./two.typ")[Second], and #link("./three.typ")[Third]."#;
        let result = remove_relative_typ_links(source);
        assert_eq!(result, r#"[First], [Second], and [Third]."#);
    }

    #[test]
    fn test_sanitize_label_name() {
        assert_eq!(pdf::sanitize_label_name("chapter 01.typ"), "chapter_01_typ");
        assert_eq!(pdf::sanitize_label_name("chapter 01"), "chapter_01");
        assert_eq!(
            pdf::sanitize_label_name("severance-01.typ"),
            "severance-01_typ"
        );
        assert_eq!(pdf::sanitize_label_name("severance-01"), "severance-01");
        assert_eq!(pdf::sanitize_label_name("my_file!@#.typ"), "my_file____typ");
        assert_eq!(pdf::sanitize_label_name("my_file!@#"), "my_file___");
    }

    #[test]
    fn test_transform_typ_links_basic() {
        let source = r#"See #link("./chapter2.typ")[next chapter]."#;
        let spine = vec![PathBuf::from("chapter1.typ"), PathBuf::from("chapter2.typ")];
        let current = PathBuf::from("chapter1.typ");
        let result = pdf::transform_typ_links_to_labels(source, &spine, &current).unwrap();
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
        let result = pdf::transform_typ_links_to_labels(source, &spine, &current).unwrap();
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
        let result = pdf::transform_typ_links_to_labels(source, &spine, &current);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found in spine"));
    }

    #[test]
    fn test_transform_typ_links_preserves_external() {
        let source = r#"Visit #link("https://example.com")[our website] or #link("mailto:test@example.com")[email us]."#;
        let spine = vec![PathBuf::from("chapter1.typ")];
        let current = PathBuf::from("chapter1.typ");
        let result = pdf::transform_typ_links_to_labels(source, &spine, &current).unwrap();
        assert_eq!(result, source); // Should be unchanged
    }

    #[test]
    fn test_transform_typ_links_preserves_fragments() {
        let source = r##"See #link("#heading")[section]."##;
        let spine = vec![PathBuf::from("chapter1.typ")];
        let current = PathBuf::from("chapter1.typ");
        let result = pdf::transform_typ_links_to_labels(source, &spine, &current).unwrap();
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
        let result = pdf::concatenate_typst_sources(&spine).unwrap();

        // Check that heading-based labels are injected (derived from filename)
        // These should appear at the start of each section
        assert!(result.contains("<chapter1>"));
        assert!(result.contains("<chapter2>"));

        // Check for metadata with labels (new format)
        assert!(
            result.contains("#metadata(\"Chapter1\") <chapter1>")
                || result.contains("#metadata(\"Chapter 1\") <chapter1>")
        );
        assert!(
            result.contains("#metadata(\"Chapter2\") <chapter2>")
                || result.contains("#metadata(\"Chapter 2\") <chapter2>")
        );

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
        let result = pdf::concatenate_typst_sources(&spine).unwrap();

        // Metadata with label should be injected (title derived from filename)
        assert!(result.starts_with("#metadata(\"Test File\") <test-file>"));
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
        let result = pdf::concatenate_typst_sources(&spine);

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
        let result = pdf::concatenate_typst_sources(&spine).unwrap();

        // Link should be transformed to label
        assert!(result.contains("#link(<chapter2>)[next chapter]"));
    }

    #[test]
    fn test_filename_to_title() {
        assert_eq!(pdf::filename_to_title("severance-ep-1"), "Severance Ep 1");
        assert_eq!(pdf::filename_to_title("my_document"), "My Document");
        assert_eq!(pdf::filename_to_title("chapter-01"), "Chapter 01");
        assert_eq!(pdf::filename_to_title("hello_world"), "Hello World");
        assert_eq!(pdf::filename_to_title("single"), "Single");
    }

    // strip_typst_markup tests moved to formats::pdf module (now private)

    #[test]
    fn test_extract_document_title_from_metadata() {
        let source = r#"#set document(title: [My Great Title])

= Chapter 1
Content here."#;

        let title = pdf::extract_document_title(source, "fallback");
        assert_eq!(title, "My Great Title");
    }

    #[test]
    fn test_extract_document_title_fallback() {
        let source = r#"= Chapter 1
Content here."#;

        let title = pdf::extract_document_title(source, "my-chapter");
        assert_eq!(title, "My Chapter");
    }

    #[test]
    fn test_extract_document_title_with_markup() {
        let source = r#"#set document(title: [Good news about hell - #emph[Severance]])"#;

        let title = pdf::extract_document_title(source, "fallback");
        // Should strip #emph and underscores
        // Note: complex nested bracket handling is limited by regex
        assert!(title.contains("Good news"));
        assert!(title.contains("Severance"));
    }

    #[test]
    fn test_extract_document_title_empty() {
        let source = r#"#set document(title: [])

Content"#;

        let title = pdf::extract_document_title(source, "default-name");
        // Empty title should fall back to filename
        assert_eq!(title, "Default Name");
    }

    #[test]
    fn test_extract_document_title_complex() {
        let source = r#"#set document(title: [Half Loop - _Severance_ [s1/e2]], author: [Test])"#;

        let title = pdf::extract_document_title(source, "fallback");
        // Should extract title and strip markup
        assert!(title.contains("Half Loop"));
        assert!(title.contains("Severance"));
    }
}

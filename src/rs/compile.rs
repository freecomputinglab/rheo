use crate::world::RheoWorld;
use std::path::PathBuf;

/// Common compilation options used across all output formats.
///
/// This struct encapsulates the core parameters needed for any compilation:
/// - Input file (the .typ file to compile)
/// - Output file (where to write the result)
/// - Root directory (for resolving imports)
/// - Optional RheoWorld (for incremental compilation)
pub struct RheoCompileOptions<'a> {
    /// The input .typ file to compile
    pub input: PathBuf,
    /// The output file path
    pub output: PathBuf,
    /// Root directory for resolving imports
    pub root: PathBuf,
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
    pub fn new(
        input: impl Into<PathBuf>,
        output: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            input: input.into(),
            output: output.into(),
            root: root.into(),
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
    /// * `world` - Mutable reference to existing RheoWorld
    pub fn incremental(
        input: impl Into<PathBuf>,
        output: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        world: &'a mut RheoWorld,
    ) -> Self {
        Self {
            input: input.into(),
            output: output.into(),
            root: root.into(),
            world: Some(world),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::formats::pdf;

    #[test]
    fn test_filename_to_title() {
        assert_eq!(pdf::filename_to_title("severance-ep-1"), "Severance Ep 1");
        assert_eq!(pdf::filename_to_title("my_document"), "My Document");
        assert_eq!(pdf::filename_to_title("chapter-01"), "Chapter 01");
        assert_eq!(pdf::filename_to_title("hello_world"), "Hello World");
        assert_eq!(pdf::filename_to_title("single"), "Single");
    }

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

use crate::world::RheoWorld;
use std::path::PathBuf;

/// Common compilation options used across all output formats.
///
/// This struct encapsulates the core parameters needed for any compilation:
/// - Input file (the .typ file to compile)
/// - Output file (where to write the result)
/// - Root directory (for resolving imports)
/// - RheoWorld (always provided by the engine)
pub struct RheoCompileOptions<'a> {
    /// The input .typ file to compile
    pub input: PathBuf,
    /// The output file path
    pub output: PathBuf,
    /// Root directory for resolving imports
    pub root: PathBuf,
    /// RheoWorld for compilation (always provided by the engine)
    pub world: &'a mut RheoWorld,
}

impl<'a> RheoCompileOptions<'a> {
    /// Create compilation options.
    ///
    /// The engine always creates and provides the World, whether in fresh
    /// or incremental mode. Plugins don't need to distinguish between these cases.
    ///
    /// # Arguments
    /// * `input` - The input .typ file to compile
    /// * `output` - The output file path
    /// * `root` - Root directory for resolving imports
    /// * `world` - Mutable reference to RheoWorld (provided by engine)
    pub fn new(
        input: impl Into<PathBuf>,
        output: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        world: &'a mut RheoWorld,
    ) -> Self {
        Self {
            input: input.into(),
            output: output.into(),
            root: root.into(),
            world,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pdf_utils;

    #[test]
    fn test_filename_to_title() {
        assert_eq!(
            pdf_utils::DocumentTitle::to_readable_name("severance-ep-1"),
            "Severance Ep 1"
        );
        assert_eq!(
            pdf_utils::DocumentTitle::to_readable_name("my_document"),
            "My Document"
        );
        assert_eq!(
            pdf_utils::DocumentTitle::to_readable_name("chapter-01"),
            "Chapter 01"
        );
        assert_eq!(
            pdf_utils::DocumentTitle::to_readable_name("hello_world"),
            "Hello World"
        );
        assert_eq!(pdf_utils::DocumentTitle::to_readable_name("single"), "Single");
    }

    #[test]
    fn test_extract_document_title_from_metadata() {
        let source = r#"#set document(title: [My Great Title])

= Chapter 1
Content here."#;

        let title = pdf_utils::DocumentTitle::from_source(source, "fallback").extract();
        assert_eq!(title, "My Great Title");
    }

    #[test]
    fn test_extract_document_title_fallback() {
        let source = r#"= Chapter 1
Content here."#;

        let title = pdf_utils::DocumentTitle::from_source(source, "my-chapter").extract();
        assert_eq!(title, "My Chapter");
    }

    #[test]
    fn test_extract_document_title_with_markup() {
        let source = r#"#set document(title: [Good news about hell - #emph[Severance]])"#;

        let title = pdf_utils::DocumentTitle::from_source(source, "fallback").extract();
        // Should strip #emph and underscores
        // Note: complex nested bracket handling is limited by regex
        assert!(title.contains("Good news"));
        assert!(title.contains("Severance"));
    }

    #[test]
    fn test_extract_document_title_empty() {
        let source = r#"#set document(title: [])

Content"#;

        let title = pdf_utils::DocumentTitle::from_source(source, "default-name").extract();
        // Empty title should fall back to filename
        assert_eq!(title, "Default Name");
    }

    #[test]
    fn test_extract_document_title_complex() {
        let source = r#"#set document(title: [Half Loop - _Severance_ [s1/e2]], author: [Test])"#;

        let title = pdf_utils::DocumentTitle::from_source(source, "fallback").extract();
        // Should extract title and strip markup
        assert!(title.contains("Half Loop"));
        assert!(title.contains("Severance"));
    }
}

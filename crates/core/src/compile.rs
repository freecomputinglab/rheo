use crate::world::RheoWorld;
use std::path::PathBuf;

/// Common compilation options used across all output formats.
///
/// This struct encapsulates the core parameters needed for any compilation:
/// - Output file (where to write the result)
/// - Root directory (for resolving imports)
/// - RheoWorld (always present for bundle mode)
///
/// # Bundle mode contract
///
/// In bundle mode, the bundle entry is a virtual file pre-populated in
/// `world.slots` (not a real path on disk). Every plugin receives a world
/// configured with the bundle entry as main. HTML and PDF plugins call
/// `typst::compile::<Bundle>(&world)` for multi-file output.
///
/// EPUB is out of scope for bundle compilation (typst-bundle has no EPUB
/// variant). The EPUB plugin creates its own per-file RheoWorld instances
/// internally and ignores `ctx.options.world`.
pub struct RheoCompileOptions<'a> {
    /// The output file path
    pub output: PathBuf,
    /// Root directory for resolving imports
    pub root: PathBuf,
    /// RheoWorld for compilation. Always present in bundle mode.
    pub world: &'a mut RheoWorld,
}

impl<'a> RheoCompileOptions<'a> {
    /// Create compilation options.
    ///
    /// # Arguments
    /// * `output` - The output file path
    /// * `root` - Root directory for resolving imports
    /// * `world` - The RheoWorld with bundle entry pre-configured
    pub fn new(
        output: impl Into<PathBuf>,
        root: impl Into<PathBuf>,
        world: &'a mut RheoWorld,
    ) -> Self {
        Self {
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
        assert_eq!(
            pdf_utils::DocumentTitle::to_readable_name("single"),
            "Single"
        );
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

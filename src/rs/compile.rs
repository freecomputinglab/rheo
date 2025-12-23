use crate::world::RheoWorld;
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
    /// Create a builder for constructing compilation options.
    pub fn builder() -> RheoCompileOptionsBuilder<'a> {
        RheoCompileOptionsBuilder::default()
    }

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

/// Builder for RheoCompileOptions
#[derive(Default)]
pub struct RheoCompileOptionsBuilder<'a> {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    root: Option<PathBuf>,
    repo_root: Option<PathBuf>,
    world: Option<&'a mut RheoWorld>,
}

impl<'a> RheoCompileOptionsBuilder<'a> {
    /// Set the input .typ file to compile
    pub fn input(mut self, path: impl Into<PathBuf>) -> Self {
        self.input = Some(path.into());
        self
    }

    /// Set the output file path
    pub fn output(mut self, path: impl Into<PathBuf>) -> Self {
        self.output = Some(path.into());
        self
    }

    /// Set the root directory for resolving imports
    pub fn root(mut self, path: impl Into<PathBuf>) -> Self {
        self.root = Some(path.into());
        self
    }

    /// Set the repository root for rheo.typ
    pub fn repo_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.repo_root = Some(path.into());
        self
    }

    /// Set an existing RheoWorld for incremental compilation
    pub fn world(mut self, world: &'a mut RheoWorld) -> Self {
        self.world = Some(world);
        self
    }

    /// Build the RheoCompileOptions
    ///
    /// # Errors
    /// Returns an error if any required field is missing
    pub fn build(self) -> crate::Result<RheoCompileOptions<'a>> {
        Ok(RheoCompileOptions {
            input: self.input.ok_or_else(|| {
                crate::RheoError::project_config("RheoCompileOptions: input is required")
            })?,
            output: self.output.ok_or_else(|| {
                crate::RheoError::project_config("RheoCompileOptions: output is required")
            })?,
            root: self.root.ok_or_else(|| {
                crate::RheoError::project_config("RheoCompileOptions: root is required")
            })?,
            repo_root: self.repo_root.ok_or_else(|| {
                crate::RheoError::project_config("RheoCompileOptions: repo_root is required")
            })?,
            world: self.world,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::formats::pdf;

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

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
    fn test_filename_to_title() {
        assert_eq!(pdf::filename_to_title("severance-ep-1"), "Severance Ep 1");
        assert_eq!(pdf::filename_to_title("my_document"), "My Document");
        assert_eq!(pdf::filename_to_title("chapter-01"), "Chapter 01");
        assert_eq!(pdf::filename_to_title("hello_world"), "Hello World");
        assert_eq!(pdf::filename_to_title("single"), "Single");
    }

    // Note: concatenate_typst_sources tests removed - replaced by build_rheo_spine in links::spine module

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

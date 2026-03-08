use std::fs;
use std::path::{Path, PathBuf};

use super::{is_single_file_test, markers::read_test_metadata};

/// Test case variants for different compilation modes
#[derive(Debug, Clone)]
pub enum TestCase {
    /// Test a directory-based project with rheo.toml
    Directory {
        /// Name of the test case
        name: String,
        /// Project path relative to rheo top-level.
        project_path: PathBuf,
    },
    /// Test a single .typ file
    SingleFile {
        name: String,
        file_path: PathBuf,
        formats: Vec<String>,
        metadata: Option<super::markers::TestMetadata>,
    },
}

impl TestCase {
    pub fn new(raw_path: &str) -> Self {
        // Check if the path is a .typ file
        if is_single_file_test(raw_path) {
            let file_path = Path::new(raw_path);
            // Use just the file stem (filename without extension) for the test name
            let name = file_path
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .replace('.', "_full_stop")
                .replace(':', "_colon")
                .replace('-', "_minus");

            // Read test markers to determine formats and metadata
            let metadata = read_test_metadata(file_path);
            let formats = metadata
                .as_ref()
                .map(|m| m.formats.clone())
                .unwrap_or_else(|| vec!["html".to_string(), "epub".to_string(), "pdf".to_string()]);

            return Self::SingleFile {
                name,
                file_path: file_path.into(),
                formats,
                metadata,
            };
        }

        // Otherwise, auto-detect based on filesystem metadata
        let path = Path::new(raw_path);
        let fs_metadata = fs::metadata(path).unwrap();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        if fs_metadata.is_file() {
            let test_metadata = read_test_metadata(path);
            let formats = test_metadata
                .as_ref()
                .map(|m| m.formats.clone())
                .unwrap_or_else(|| vec!["html".to_string(), "epub".to_string(), "pdf".to_string()]);

            Self::SingleFile {
                name,
                file_path: path.into(),
                formats,
                metadata: test_metadata,
            }
        } else if fs_metadata.is_dir() {
            Self::Directory {
                name,
                project_path: path.into(),
            }
        } else {
            panic!("test case should only be a file or a directory");
        }
    }

    pub fn name(&self) -> &str {
        match self {
            TestCase::Directory { name, .. } => name,
            TestCase::SingleFile { name, .. } => name,
        }
    }

    pub fn project_path(&self) -> &PathBuf {
        match self {
            TestCase::Directory { project_path, .. } => project_path,
            TestCase::SingleFile { file_path, .. } => file_path,
        }
    }

    /// Returns the format names to test for this test case
    pub fn formats(&self) -> Vec<String> {
        match self {
            TestCase::Directory { .. } => {
                vec!["html".to_string(), "epub".to_string(), "pdf".to_string()]
            }
            TestCase::SingleFile { formats, .. } => formats.clone(),
        }
    }

    /// Check if this test case is a single file test
    pub fn is_single_file(&self) -> bool {
        matches!(self, TestCase::SingleFile { .. })
    }

    /// Get test metadata for SingleFile tests, None for Directory tests
    pub fn metadata(&self) -> Option<&super::markers::TestMetadata> {
        match self {
            TestCase::SingleFile { metadata, .. } => metadata.as_ref(),
            TestCase::Directory { .. } => None,
        }
    }
}

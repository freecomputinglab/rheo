use rheo::OutputFormat;
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
    #[allow(dead_code)]
    SingleFile {
        name: String,
        file_path: PathBuf,
        formats: Vec<OutputFormat>,
    },
}

impl TestCase {
    pub fn new(raw_path: &str) -> Self {
        // Check if the path is a .typ file
        if is_single_file_test(raw_path) {
            let file_path = Path::new(raw_path);
            let name = file_path
                .to_string_lossy()
                .replace('/', "_slash")
                .replace(".typ", "");

            // Read test markers to determine formats
            let formats = read_test_metadata(file_path)
                .map(|metadata| {
                    metadata
                        .formats
                        .iter()
                        .filter_map(|f| match f.as_str() {
                            "html" => Some(OutputFormat::Html),
                            "pdf" => Some(OutputFormat::Pdf),
                            "epub" => Some(OutputFormat::Epub),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_else(OutputFormat::all_variants);

            return Self::SingleFile {
                name,
                file_path: file_path.into(),
                formats,
            };
        }

        // Otherwise, auto-detect based on filesystem metadata
        let path = Path::new(raw_path);
        let metadata = fs::metadata(path).unwrap();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        if metadata.is_file() {
            let formats = read_test_metadata(path)
                .map(|metadata| {
                    metadata
                        .formats
                        .iter()
                        .filter_map(|f| match f.as_str() {
                            "html" => Some(OutputFormat::Html),
                            "pdf" => Some(OutputFormat::Pdf),
                            "epub" => Some(OutputFormat::Epub),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_else(OutputFormat::all_variants);

            Self::SingleFile {
                name,
                file_path: path.into(),
                formats,
            }
        } else if metadata.is_dir() {
            Self::Directory {
                name,
                project_path: path.into(),
            }
        } else {
            panic!("test case should only be a file or a directory")
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

    /// Returns the file path for SingleFile tests, or None for Directory tests
    #[allow(unused)]
    pub fn file_path(&self) -> Option<&PathBuf> {
        match self {
            TestCase::Directory { .. } => None,
            TestCase::SingleFile { file_path, .. } => Some(file_path),
        }
    }

    /// Returns the project root directory for the test case
    #[allow(unused)]
    pub fn project_root(&self) -> PathBuf {
        match self {
            TestCase::Directory { project_path, .. } => project_path.clone(),
            TestCase::SingleFile { file_path, .. } => {
                // For single files, use parent directory as project root
                file_path.parent().unwrap_or(Path::new(".")).to_path_buf()
            }
        }
    }

    /// Returns the formats to test for this test case
    pub fn formats(&self) -> Vec<OutputFormat> {
        match self {
            TestCase::Directory { .. } => OutputFormat::all_variants(),
            TestCase::SingleFile { formats, .. } => formats.clone(),
        }
    }

    /// Check if this test case is a single file test
    pub fn is_single_file(&self) -> bool {
        matches!(self, TestCase::SingleFile { .. })
    }
}

/// Set up test environment (e.g., create temp directories)
#[allow(dead_code)]
pub fn setup_test_environment() -> PathBuf {
    let test_store = PathBuf::from("tests/store");
    std::fs::create_dir_all(&test_store).expect("Failed to create tests/store");
    test_store
}

/// Clean up test environment after tests complete
#[allow(dead_code)]
pub fn cleanup_test_environment(path: &PathBuf) {
    if path.exists() {
        std::fs::remove_dir_all(path).ok();
    }
}

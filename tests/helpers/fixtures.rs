use rheo::OutputFormat;
use std::fs;
use std::path::{Path, PathBuf};

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
        let path = Path::new(raw_path);
        let metadata = fs::metadata(path).unwrap();
        let name = path.file_stem().unwrap().to_str().unwrap().to_string();
        if metadata.is_file() {
            Self::SingleFile {
                name,
                file_path: path.into(),
                formats: OutputFormat::all_variants(),
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

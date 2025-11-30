use crate::helpers::fixtures::TestCase;
use glob::glob;

/// Discover all directory-based test projects
/// Searches for examples/*/rheo.toml patterns
pub fn discover_directory_tests() -> Vec<TestCase> {
    let mut tests = Vec::new();

    // Find all rheo.toml files in examples/
    let pattern = "examples/*/rheo.toml";
    match glob(pattern) {
        Ok(paths) => {
            for entry in paths.flatten() {
                if let Some(parent) = entry.parent() {
                    let name = parent
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    tests.push(TestCase::Directory {
                        name,
                        project_path: parent.to_path_buf(),
                    });
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to discover directory tests: {}", e);
        }
    }

    tests
}

/// Discover single-file test cases with markers
/// Looks for .typ files containing '// rheo-test: single-file'
#[allow(dead_code)]
pub fn discover_single_file_tests() -> Vec<TestCase> {
    let mut tests = Vec::new();

    // Find all .typ files in examples/
    let pattern = "examples/**/*.typ";
    match glob(pattern) {
        Ok(paths) => {
            for entry in paths.flatten() {
                if let Ok(content) = std::fs::read_to_string(&entry) {
                    // Check for single-file marker
                    if content.contains("// rheo-test: single-file") {
                        let name = entry
                            .file_stem()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unknown")
                            .to_string();

                        // Parse formats from metadata
                        let formats = parse_test_formats(&content);

                        tests.push(TestCase::SingleFile {
                            name,
                            file_path: entry,
                            formats,
                        });
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to discover single-file tests: {}", e);
        }
    }

    tests
}

/// Parse format metadata from test file comments
/// Looks for '// rheo-test: formats=pdf,html'
#[allow(dead_code)]
fn parse_test_formats(content: &str) -> Vec<String> {
    for line in content.lines() {
        if let Some(formats_str) = line.strip_prefix("// rheo-test: formats=") {
            return formats_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }
    }
    // Default to all formats if not specified
    vec!["pdf".to_string(), "html".to_string()]
}

pub mod comparison;
pub mod fixtures;
pub mod markers;
pub mod reference;

/// Determines if a test path represents a single .typ file
pub fn is_single_file_test(path: &str) -> bool {
    path.ends_with(".typ")
}

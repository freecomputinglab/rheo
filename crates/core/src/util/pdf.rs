//! PDF utility functions shared across rheo core and plugins.

/// Sanitize a filename to create a valid Typst label name.
///
/// Replaces non-alphanumeric characters (except hyphens and underscores) with underscores.
pub fn sanitize_label_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Filename-to-title helper.
///
/// Document titles are read from `#set document(title: …)` via the AST-based
/// [`DocumentMetadata`](crate::parser::DocumentMetadata) extractor; this type
/// only carries the filename fallback used when a vertebra sets no title.
pub struct DocumentTitle;

impl DocumentTitle {
    /// Convert a filename to a readable title.
    ///
    /// Transforms a filename stem into a human-readable title by replacing
    /// separators with spaces and capitalizing words.
    ///
    /// # Arguments
    /// * `filename` - The filename to convert
    ///
    /// # Returns
    /// A title-cased version of the filename
    pub fn to_readable_name(filename: &str) -> String {
        filename
            .replace(['-', '_'], " ")
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_label_name() {
        assert_eq!(sanitize_label_name("chapter 01"), "chapter_01");
        assert_eq!(sanitize_label_name("severance-01"), "severance-01");
        assert_eq!(sanitize_label_name("my_file!@#"), "my_file___");
        assert_eq!(sanitize_label_name("test.typ"), "test_typ");
    }

    #[test]
    fn test_filename_to_title() {
        assert_eq!(
            DocumentTitle::to_readable_name("severance-ep-1"),
            "Severance Ep 1"
        );
        assert_eq!(
            DocumentTitle::to_readable_name("my_document"),
            "My Document"
        );
        assert_eq!(DocumentTitle::to_readable_name("chapter-01"), "Chapter 01");
        assert_eq!(
            DocumentTitle::to_readable_name("hello_world"),
            "Hello World"
        );
        assert_eq!(DocumentTitle::to_readable_name("single"), "Single");
    }
}

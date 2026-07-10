/// PDF utility functions shared across rheo core and plugins
use crate::util::constants::TYPST_LABEL_PATTERN;

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

/// Document title extractor that parses Typst source for title metadata.
///
/// Provides methods for extracting document titles from Typst source code or
/// generating readable titles from filenames.
pub struct DocumentTitle {
    source: String,
    fallback_filename: String,
}

impl DocumentTitle {
    /// Create a DocumentTitle from source code and a fallback filename.
    ///
    /// # Arguments
    /// * `source` - Typst source code to extract title from
    /// * `fallback` - Filename to use if no title is found in source
    pub fn from_source(source: impl Into<String>, fallback: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            fallback_filename: fallback.into(),
        }
    }

    /// Extract the document title.
    ///
    /// Searches for `#set document(title: [...])` in the source and extracts the content.
    /// Falls back to the filename converted to title case if no title is found.
    pub fn extract(&self) -> String {
        // Find the start of the title parameter
        if let Some(title_start) = self.source.find("#set document(") {
            let after_doc = &self.source[title_start..];
            if let Some(title_pos) = after_doc.find("title:") {
                let after_title = &after_doc[title_pos + 6..]; // Skip "title:"

                // Find the opening bracket for the title
                // PDF metadata uses bracket-delimited format: /Title [(title text)]
                if let Some(bracket_start) = after_title.find('[') {
                    let title_content = &after_title[bracket_start + 1..];

                    // Count brackets to find the matching closing bracket
                    // Handles nested brackets like: [(Chapter [1])]
                    // Algorithm:
                    // 1. Start with depth=1 (for the opening bracket we just found)
                    // 2. Scan forward, incrementing depth for '[', decrementing for ']'
                    // 3. When depth reaches 0, we've found the matching closing bracket
                    let mut depth = 1;
                    let mut end_pos = 0;

                    for (i, ch) in title_content.chars().enumerate() {
                        if ch == '[' {
                            depth += 1; // Found nested opening bracket
                        } else if ch == ']' {
                            depth -= 1; // Found closing bracket
                            if depth == 0 {
                                // This is the matching closing bracket
                                end_pos = i;
                                break;
                            }
                        }
                    }

                    if end_pos > 0 {
                        let title = &title_content[..end_pos];
                        // Strip Typst markup for plain text
                        let cleaned = strip_typst_markup(title);
                        if !cleaned.trim().is_empty() {
                            return cleaned;
                        }
                    }
                }
            }
        }

        // Fallback: use filename, convert to title case
        Self::to_readable_name(&self.fallback_filename)
    }

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

/// Strip basic Typst markup to get plain text.
///
/// Removes common Typst markup patterns like #emph[...], #strong[...],
/// and italic markers (_) to extract plain text from formatted content.
fn strip_typst_markup(text: &str) -> String {
    // Remove #emph[...], #strong[...], etc.
    let result = TYPST_LABEL_PATTERN.replace_all(text, "$1");

    // Remove underscores (italic markers)
    let result = result.replace('_', "");

    result.trim().to_string()
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

    #[test]
    fn test_extract_document_title_from_metadata() {
        let source = r#"#set document(title: [My Great Title])

= Chapter 1
Content here."#;

        let title = DocumentTitle::from_source(source, "fallback").extract();
        assert_eq!(title, "My Great Title");
    }

    #[test]
    fn test_extract_document_title_fallback() {
        let source = r#"= Chapter 1
Content here."#;

        let title = DocumentTitle::from_source(source, "my-chapter").extract();
        assert_eq!(title, "My Chapter");
    }

    #[test]
    fn test_extract_document_title_with_markup() {
        let source = r#"#set document(title: [Good news about hell - #emph[Severance]])"#;

        let title = DocumentTitle::from_source(source, "fallback").extract();
        // Should strip #emph and underscores
        // Note: complex nested bracket handling is limited by regex
        assert!(title.contains("Good news"));
        assert!(title.contains("Severance"));
    }

    #[test]
    fn test_extract_document_title_empty() {
        let source = r#"#set document(title: [])

Content"#;

        let title = DocumentTitle::from_source(source, "default-name").extract();
        // Empty title should fall back to filename
        assert_eq!(title, "Default Name");
    }

    #[test]
    fn test_extract_document_title_complex() {
        let source = r#"#set document(title: [Half Loop - _Severance_ [s1/e2]], author: [Test])"#;

        let title = DocumentTitle::from_source(source, "fallback").extract();
        // Should extract title and strip markup
        assert!(title.contains("Half Loop"));
        assert!(title.contains("Severance"));
    }
}

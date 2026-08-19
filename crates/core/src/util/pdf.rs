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
/// The real document title is read from `#set document(title: …)` by Typst
/// itself, post-compile (see `crate::plugins::document_meta::DocumentMeta`);
/// this type only carries the filename-derived fallback used pre-compile, and
/// when a vertebra's output has no resolved title at all.
pub struct DocumentTitle;

impl DocumentTitle {
    /// Convert a filename stem to a title-cased, human-readable name:
    /// separators become spaces, each word is capitalized.
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

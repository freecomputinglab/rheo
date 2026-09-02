//! PDF utility functions shared across rheo core and plugins.

/// Filename-to-title helper.
///
/// The real document title is read from `#set document(title: …)` by Typst
/// itself, post-compile (see `crate::reticulate::document_meta::DocumentMeta`);
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

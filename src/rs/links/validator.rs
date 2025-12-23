use std::path::{Path, PathBuf};
use typst::diag::{EcoString, Severity, SourceDiagnostic};
use crate::links::types::LinkInfo;

/// Validate links and return diagnostic warnings for broken ones
pub fn validate_links(
    links: &[LinkInfo],
    project_root: &Path,
    source_file: &Path,
) -> Vec<SourceDiagnostic> {
    links.iter()
        .filter_map(|link| validate_single_link(link, project_root, source_file))
        .collect()
}

fn validate_single_link(
    link: &LinkInfo,
    project_root: &Path,
    source_file: &Path,
) -> Option<SourceDiagnostic> {
    // Only validate relative .typ links
    if !is_relative_typ_link(&link.url) {
        return None;
    }

    // Resolve relative path
    let target = resolve_relative_path(source_file, &link.url);

    // Check if file exists
    if !target.exists() {
        let msg = format!(
            "Link target does not exist: {} (resolved to: {})",
            link.url,
            target.display()
        );

        // Create warning diagnostic with span
        return Some(SourceDiagnostic {
            span: link.span,
            message: EcoString::from(msg),
            severity: Severity::Warning,
            hints: Default::default(),
            trace: Default::default(),
        });
    }

    // Optionally check if target is within project root
    if let Ok(canonical_target) = target.canonicalize() {
        if let Ok(canonical_root) = project_root.canonicalize() {
            if !canonical_target.starts_with(&canonical_root) {
                let msg = format!(
                    "Link target is outside project root: {}",
                    link.url
                );
                return Some(SourceDiagnostic {
                    span: link.span,
                    message: EcoString::from(msg),
                    severity: Severity::Warning,
                    hints: Default::default(),
                    trace: Default::default(),
                });
            }
        }
    }

    None
}

fn is_relative_typ_link(url: &str) -> bool {
    // Check if URL is:
    // 1. Ends with .typ
    // 2. Not an external URL (http://, https://, mailto:, etc.)
    // 3. Not a fragment-only link (#anchor)

    if !url.ends_with(".typ") {
        return false;
    }

    if url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.starts_with("//")
        || url.starts_with('#')
    {
        return false;
    }

    true
}

fn resolve_relative_path(source_file: &Path, url: &str) -> PathBuf {
    // Resolve relative path from source file location
    let source_dir = source_file.parent().unwrap_or(Path::new("."));
    source_dir.join(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use typst::syntax::Span;

    fn create_test_link(url: &str) -> LinkInfo {
        LinkInfo {
            url: url.to_string(),
            body: "test".to_string(),
            span: Span::detached(),
            byte_range: 0..0,
        }
    }

    #[test]
    fn test_is_relative_typ_link() {
        assert!(is_relative_typ_link("./chapter1.typ"));
        assert!(is_relative_typ_link("../other.typ"));
        assert!(is_relative_typ_link("file.typ"));

        assert!(!is_relative_typ_link("https://example.com/file.typ"));
        assert!(!is_relative_typ_link("http://example.com"));
        assert!(!is_relative_typ_link("mailto:test@example.com"));
        assert!(!is_relative_typ_link("#anchor"));
        assert!(!is_relative_typ_link("./file.md"));
    }

    #[test]
    fn test_resolve_relative_path() {
        let source = Path::new("/project/src/chapter1.typ");
        let resolved = resolve_relative_path(source, "./chapter2.typ");
        assert_eq!(resolved, Path::new("/project/src/./chapter2.typ"));

        let resolved = resolve_relative_path(source, "../other.typ");
        assert_eq!(resolved, Path::new("/project/src/../other.typ"));
    }

    #[test]
    fn test_validate_links_skip_external() {
        let links = vec![
            create_test_link("https://example.com"),
            create_test_link("#anchor"),
            create_test_link("./file.md"),
        ];

        let diagnostics = validate_links(
            &links,
            Path::new("/project"),
            Path::new("/project/src/file.typ"),
        );

        assert_eq!(diagnostics.len(), 0, "External URLs should be skipped");
    }
}

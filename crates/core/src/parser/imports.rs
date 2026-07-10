//! Extractor: `#import`/`#include` package paths.

use super::{SyntaxSite, WalkCtx};
use std::ops::Range;
use typst::syntax::{Source, SyntaxKind, SyntaxNode};

/// A path string extracted from an `#import`/`#include` statement.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The raw path string (e.g. `./utils.typ` or `@preview/foo:0.1.0`).
    pub path: String,
    /// Byte range of the path string (not the whole statement).
    pub byte_range: Range<usize>,
    /// `true` if the path starts with `@` (a package import).
    pub is_package: bool,
}

impl SyntaxSite for ImportInfo {
    fn visit(
        _source: &Source,
        node: &SyntaxNode,
        offset: usize,
        _ctx: WalkCtx,
        out: &mut Vec<Self>,
    ) {
        if matches!(
            node.kind(),
            SyntaxKind::ModuleImport | SyntaxKind::ModuleInclude
        ) && let Some(info) = parse_import_node(node, offset)
        {
            out.push(info);
        }
    }
}

/// Parse a `ModuleImport`/`ModuleInclude` (starting at byte `node_offset`) into
/// `ImportInfo`, reading the first `Str` child as the path and deriving its byte
/// range from the walker-supplied offset.
fn parse_import_node(node: &SyntaxNode, node_offset: usize) -> Option<ImportInfo> {
    let mut offset = node_offset;
    for child in node.children() {
        if child.kind() == SyntaxKind::Str {
            let path = child.leaf_text().trim_matches('"').to_string();
            let byte_range = offset..offset + child.len();
            return Some(ImportInfo {
                is_package: path.starts_with('@'),
                path,
                byte_range,
            });
        }
        offset += child.len();
    }
    None
}

/// Extract package import paths (those starting with `@`) from Typst source.
pub fn extract_package_imports(source: &Source) -> Vec<String> {
    ImportInfo::collect(source)
        .into_iter()
        .filter(|info| info.is_package)
        .map(|info| info.path)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_package_imports() {
        let source = Source::detached(r#"#import "@preview/tablex:0.0.6": tablex"#);
        let imports = extract_package_imports(&source);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0], "@preview/tablex:0.0.6");
    }

    #[test]
    fn test_extract_package_imports_multiple() {
        let source = Source::detached(
            r#"#import "@preview/foo:1.0.0": *
#import "./local.typ": utils
#import "@preview/bar:2.0.0": bar"#,
        );
        let imports = extract_package_imports(&source);
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0], "@preview/foo:1.0.0");
        assert_eq!(imports[1], "@preview/bar:2.0.0");
    }
}

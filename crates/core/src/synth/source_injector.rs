//! What rheo wraps around a file's own Typst before the compiler sees it.
//!
//! Two shapes, one per kind of file in a bundle: the synthesized main gets the
//! `rheo.typ` template, the bundle-root metadata helpers and the plugin's own
//! library; every other file gets the `target()` polyfill and its own
//! per-vertebra prelude/epilogue. Both are composed as [`TypstStmt`]s and
//! rendered once, so the syntax lives in [`super::typst_source`] rather than in
//! `format!` calls at the injection site.

use crate::reticulate::VertebraInjection;
use crate::synth::typst_source::{TypstBlock, TypstStmt};
use std::collections::HashMap;

/// Makes `target()` return rheo's output format, so an authored file detects
/// the format the same way under every plugin. Opaque source, hence `Raw`.
const TARGET_POLYFILL: &str = "// Polyfill target() to return rheo's output format from sys.inputs\n\
     #let target() = if \"rheo-context\" in sys.inputs and \"target\" in sys.inputs.rheo-context { sys.inputs.rheo-context.target } else { std.target() }";

/// The Typst rheo injects around one compile's files.
pub struct SourceInjector<'a> {
    /// Whether this compile targets a rheo output format. Only then is the
    /// `target()` polyfill injected; native PDF keeps Typst's own `target()`.
    polyfill_target: bool,
    /// The plugin's own Typst library, spliced into the main file.
    plugin_library: Option<&'a str>,
    /// Per-vertebra prelude/epilogue, keyed by the file's bundle-relative path.
    injections: &'a HashMap<String, VertebraInjection>,
}

impl<'a> SourceInjector<'a> {
    pub fn new(
        polyfill_target: bool,
        plugin_library: Option<&'a str>,
        injections: &'a HashMap<String, VertebraInjection>,
    ) -> Self {
        Self {
            polyfill_target,
            plugin_library,
            injections,
        }
    }

    /// The bundle main: polyfill, the `rheo.typ` template, the bundle-root
    /// metadata helpers, the plugin library, the template application, then the
    /// synthesized body.
    ///
    /// `rheo-metadata-all` and `rheo-handle-title` are main-only: a single
    /// vertebra never needs every vertebra's metadata at once, nor an anchor's
    /// title lookup (anchors only appear in bundle-root `#document(...)`
    /// bodies). No format gate is needed beyond the polyfill's — marrow and
    /// beacons are only ever assembled for per-page targets anyway.
    pub fn main(&self, body: &str) -> String {
        let stmts = vec![
            self.polyfill(),
            TypstStmt::Raw(include_str!("../typ/rheo.typ").to_string()),
            TypstStmt::MetadataHelper,
            TypstStmt::MetadataAllHelper,
            TypstStmt::HandleTitleHelper,
            TypstStmt::Raw(self.plugin_library.unwrap_or_default().to_string()),
            TypstStmt::Raw("#show: rheo_template".to_string()),
        ];
        format!("{}\n\n{body}", TypstBlock(stmts))
    }

    /// A vertebra or partial identified by its bundle-relative path: the
    /// polyfill, that vertebra's `rheo-context()` prelude, its own source, then
    /// its metadata-beacon epilogue. A file with neither (a partial pulled in by
    /// an `#include`) is returned untouched but for the polyfill.
    pub fn vertebra(&self, rel_path: &str, body: &str) -> String {
        let injection = self.injections.get(rel_path);
        let prelude = injection.map(|i| i.prelude.as_str()).unwrap_or_default();
        let epilogue = injection.map(|i| i.epilogue.as_str()).unwrap_or_default();
        if !self.polyfill_target && prelude.is_empty() && epilogue.is_empty() {
            return body.to_string();
        }
        let polyfill = TypstBlock(vec![self.polyfill()]).to_string();
        let head = match polyfill.is_empty() {
            true => polyfill,
            false => polyfill + "\n\n",
        };
        format!("{head}{prelude}{body}{epilogue}")
    }

    fn polyfill(&self) -> TypstStmt {
        TypstStmt::Raw(match self.polyfill_target {
            true => TARGET_POLYFILL.to_string(),
            false => String::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn injections(entries: &[(&str, VertebraInjection)]) -> HashMap<String, VertebraInjection> {
        entries
            .iter()
            .map(|(path, inj)| (path.to_string(), inj.clone()))
            .collect()
    }

    #[test]
    fn main_carries_template_helpers_and_plugin_library() {
        let none = injections(&[]);
        let injector = SourceInjector::new(true, Some("#let plugin-lib = 1"), &none);
        let out = injector.main("#document(\"a.html\")[]");

        assert!(out.starts_with("// Polyfill target()"));
        assert!(out.contains("#import \"/typ/metadata.typ\": rheo-metadata-all"));
        assert!(out.contains("#let plugin-lib = 1"));
        assert!(out.ends_with("#show: rheo_template\n\n#document(\"a.html\")[]"));
    }

    /// PDF compiles with Typst's own `target()`, so nothing is injected for it;
    /// an absent plugin library must not leave a blank run behind either.
    #[test]
    fn main_without_polyfill_or_plugin_library_leaves_no_gap() {
        let none = injections(&[]);
        let out = SourceInjector::new(false, None, &none).main("body");

        assert!(!out.contains("Polyfill target()"));
        assert!(
            out.starts_with("//"),
            "template comes first: {}",
            &out[..40]
        );
        assert!(out.ends_with("rheo-handle-title\n\n#show: rheo_template\n\nbody"));
    }

    #[test]
    fn vertebra_wraps_its_own_prelude_and_epilogue() {
        let map = injections(&[(
            "content/a.typ",
            VertebraInjection {
                prelude: "#let rheo-context() = ()\n\n".to_string(),
                epilogue: "\n#beacon\n".to_string(),
            },
        )]);
        let out = SourceInjector::new(true, None, &map).vertebra("content/a.typ", "= Title");

        assert!(out.contains("#let rheo-context() = ()\n\n= Title\n#beacon\n"));
        assert!(out.starts_with("// Polyfill target()"));
    }

    /// A partial with no injection under a format that needs no polyfill is
    /// handed to Typst exactly as authored.
    #[test]
    fn untouched_file_is_returned_verbatim() {
        let none = injections(&[]);
        let out = SourceInjector::new(false, None, &none).vertebra("content/partial.typ", "= P");
        assert_eq!(out, "= P");
    }
}

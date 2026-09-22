//! What rheo wraps around a file's own Typst before the compiler sees it.
//!
//! Two shapes, one per kind of file in a bundle: the synthesized main gets the
//! `rheo.typ` template, the bundle-root metadata helpers and the plugin's own
//! library; every other file gets the `target()` polyfill and its own
//! per-vertebra prelude/epilogue. Both are composed as [`TypstStmt`]s and
//! rendered once, so the syntax lives in [`super::typst_source`] rather than in
//! `format!` calls at the injection site.

use crate::reticulate::VertebraInjection;
use crate::synth::source_map::{AuthoredFile, SourceMap};
use crate::synth::typst_source::{TypstBlock, TypstStmt};
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Arc;

/// A synthesized Typst source plus the map back to the authored text it
/// carries.
pub struct Synthesized {
    pub text: String,
    pub map: SourceMap,
}

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
    pub fn main(&self, body: &str) -> Synthesized {
        let stmts = vec![
            self.polyfill(),
            TypstStmt::Raw(include_str!("../typ/rheo.typ").to_string()),
            TypstStmt::MetadataHelper,
            TypstStmt::MetadataAllHelper,
            TypstStmt::HandleTitleHelper,
            TypstStmt::Raw(self.plugin_library.unwrap_or_default().to_string()),
            TypstStmt::Raw("#show: rheo_template".to_string()),
        ];
        let text = format!("{}\n\n{body}", TypstBlock(stmts));
        let mut map = SourceMap::default();
        map.push_injected(text.len());
        Synthesized { text, map }
    }

    /// A vertebra or partial identified by its bundle-relative path: the
    /// polyfill, that vertebra's `rheo-context()` prelude, its own source, then
    /// its metadata-beacon epilogue. A file with neither (a partial pulled in by
    /// an `#include`) is returned untouched but for the polyfill.
    pub fn vertebra(&self, rel_path: &str, body: &str) -> Synthesized {
        let injection = self.injections.get(rel_path);
        let generated = injection.map(|i| i.generated.as_str()).unwrap_or_default();
        let project_prelude = injection.and_then(|i| i.project_prelude.as_ref());
        let epilogue = injection.map(|i| i.epilogue.as_str()).unwrap_or_default();
        let file = || AuthoredFile {
            name: rel_path.to_string(),
            text: body.into(),
        };
        if injection.is_none() && !self.polyfill_target {
            let mut map = SourceMap::default();
            map.push_authored(file(), 0, body.len());
            return Synthesized {
                text: body.to_string(),
                map,
            };
        }
        let polyfill = TypstBlock(vec![self.polyfill()]).to_string();
        let head = match polyfill.is_empty() {
            true => polyfill,
            false => polyfill + "\n\n",
        };
        // `generated`, a literal two-newline separator, then the project's own
        // `[spine] prelude` verbatim (as spliced, i.e. with its own trailing
        // two-newline separator) when configured.
        const SEPARATOR: &str = "\n\n";
        let project_splice = project_prelude
            .map(|(_, text)| format!("{text}{SEPARATOR}"))
            .unwrap_or_default();

        let mut map = SourceMap::default();
        map.push_injected(head.len() + generated.len() + SEPARATOR.len());
        if let Some((path, text)) = project_prelude {
            map.push_authored(
                AuthoredFile {
                    name: path.clone(),
                    text: text.clone(),
                },
                0,
                text.len(),
            );
            map.push_injected(SEPARATOR.len());
        }
        map.push_authored(file(), 0, body.len());
        map.push_injected(epilogue.len());
        Synthesized {
            text: format!("{head}{generated}{SEPARATOR}{project_splice}{body}{epilogue}"),
            map,
        }
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
        let out = injector.main("#document(\"a.html\")[]").text;

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
        let out = SourceInjector::new(false, None, &none).main("body").text;

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
                generated: "#let rheo-context() = ()".to_string(),
                project_prelude: None,
                epilogue: "\n#beacon\n".to_string(),
            },
        )]);
        let out = SourceInjector::new(true, None, &map)
            .vertebra("content/a.typ", "= Title")
            .text;

        assert!(out.contains("#let rheo-context() = ()\n\n= Title\n#beacon\n"));
        assert!(out.starts_with("// Polyfill target()"));
    }

    #[test]
    fn vertebra_map_resolves_project_prelude_offset_to_its_own_file() {
        let map = injections(&[(
            "content/a.typ",
            VertebraInjection {
                generated: "#let rheo-context() = ()".to_string(),
                project_prelude: Some((
                    "content/_lib/prelude.typ".to_string(),
                    Arc::from("#let broken = undefined-thing"),
                )),
                epilogue: String::new(),
            },
        )]);
        let synthesized =
            SourceInjector::new(true, None, &map).vertebra("content/a.typ", "= Title");

        let prelude_offset = synthesized.text.find("undefined-thing").unwrap();
        let (file, range) = synthesized
            .map
            .resolve(&(prelude_offset..prelude_offset + "undefined-thing".len()))
            .expect("resolves");
        assert_eq!(file.name, "content/_lib/prelude.typ");
        assert_eq!(
            range,
            "#let broken = undefined-thing"
                .find("undefined-thing")
                .unwrap().."#let broken = undefined-thing".len()
        );

        let body_offset = synthesized.text.find("Title").unwrap();
        let (file, range) = synthesized
            .map
            .resolve(&(body_offset..body_offset + "Title".len()))
            .expect("resolves");
        assert_eq!(file.name, "content/a.typ");
        assert_eq!(range, "= Title".find("Title").unwrap()..7);
    }

    /// A partial with no injection under a format that needs no polyfill is
    /// handed to Typst exactly as authored.
    #[test]
    fn untouched_file_is_returned_verbatim() {
        let none = injections(&[]);
        let out = SourceInjector::new(false, None, &none)
            .vertebra("content/partial.typ", "= P")
            .text;
        assert_eq!(out, "= P");
    }

    /// A vertebra with a polyfill and a prelude resolves a byte offset of the
    /// body back to the body's own offset.
    #[test]
    fn vertebra_map_resolves_body_offset_to_the_body_file() {
        let map = injections(&[(
            "content/a.typ",
            VertebraInjection {
                generated: "#let rheo-context() = ()".to_string(),
                project_prelude: None,
                epilogue: "\n#beacon\n".to_string(),
            },
        )]);
        let synthesized =
            SourceInjector::new(true, None, &map).vertebra("content/a.typ", "= Title");

        let offset = synthesized.text.find("Title").unwrap();
        let (file, range) = synthesized
            .map
            .resolve(&(offset..offset + "Title".len()))
            .expect("resolves");
        assert_eq!(file.name, "content/a.typ");
        assert_eq!(range, "= Title".find("Title").unwrap()..7);
    }
}

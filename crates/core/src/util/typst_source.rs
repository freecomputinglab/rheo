//! A minimal Typst *statement* model for the source rheo synthesizes.
//!
//! Where [`TypstLiteral`](super::typst_literal::TypstLiteral) models a Typst
//! *value*, [`TypstStmt`] models a top-level *statement* rheo injects into the
//! compiled bundle — a `#let` prelude, a per-page `state().update()`, a
//! `#document(..)[..]` wrapper, a cross-vertebra handle anchor, an `#include`.
//! Assembling the synthesized source from these typed statements keeps the
//! Typst syntax and escaping in one place instead of hand-written, separately
//! escaped strings at each injection site.
//!
//! Opaque source rheo does not construct field-by-field — the `rheo.typ`
//! template, the `target()` polyfill, a vertebra's own authored text — is not
//! modeled here; it rides as [`TypstStmt::Raw`] when it needs to sit in a
//! statement list.

use crate::util::path::escape_typst_content;
use crate::util::typst_literal::TypstLiteral;
use std::fmt;

/// A synthesized top-level Typst statement, rendered via [`fmt::Display`].
///
/// `Display` emits the bare statement — no surrounding indentation or trailing
/// newline — so callers control layout. [`TypstStmt::Document`] is the one
/// exception: it renders its own multi-line block (header, indented body,
/// closing `]`).
pub enum TypstStmt {
    /// Verbatim Typst source (template blobs, polyfills, `#show: …` application).
    Raw(String),
    /// `#let <name> = <value>`.
    Let { name: String, value: TypstLiteral },
    /// The per-vertebra `rheo-context` binding, as a zero-arg function that
    /// composes this file's `handle` with the format-global values spread from
    /// `sys.inputs.rheo-context`, plus a `metadata-of` closure (see
    /// [`TypstStmt::MetadataHelper`]) any vertebra can call to read another
    /// vertebra's beacon:
    /// `#let rheo-context() = (handle: "<handle>", metadata-of: rheo-metadata, ..sys.inputs.rheo-context)`.
    /// The function form lets authors mock it under vanilla Typst; the spread
    /// keeps the (potentially large) `spine` stored once in `sys.inputs` rather
    /// than baked into every vertebra. `sys.inputs` reads need no `#context`, so
    /// authors still read `rheo-context().handle` directly. Since `metadata-of`
    /// is a dict field rather than a method, calling it needs the awkward but
    /// necessary `(rheo-context().metadata-of)("some-handle")` form, not
    /// `rheo-context().metadata-of("some-handle")`.
    ContextBinding { handle: String },
    /// The `rheo-metadata` helper function, defined once per vertebra ahead of
    /// [`TypstStmt::ContextBinding`] (which references it as `metadata-of`).
    /// Reads another vertebra's [`TypstStmt::MetadataBeacon`] via `query()`,
    /// returning its published fields as a dict (or `(:)` if no beacon with
    /// that handle was found — e.g. under a `SingleCombined` layout, where no
    /// beacon is emitted at all).
    MetadataHelper,
    /// A per-vertebra metadata "beacon": a labelled, hidden `#metadata(...)`
    /// element publishing this vertebra's own resolved `#set document(...)`
    /// values (`title`/`author`/`description`/`keywords`/`date`), queryable by
    /// any other vertebra in the same bundle compile via
    /// [`TypstStmt::MetadataHelper`]'s `rheo-metadata`. Emitted as a vertebra
    /// *epilogue* (after the vertebra's own body), never injected after the
    /// synthesized bundle main's `#include` — a `set document(...)` rule
    /// inside an `#include`d module does not leak to bundle-root siblings, but
    /// `document.title`/etc. do see it via Typst's realization/introspection.
    /// Only emitted for `OnePerVertebra` layouts (HTML/EPUB); combined PDF
    /// leaks cross-vertebra `set document(...)` state within its one shared
    /// `#document(...)`, so no beacon is emitted there.
    MetadataBeacon { handle: String },
    /// `#state("<key>").update(<value>)`.
    StateUpdate { key: String, value: TypstLiteral },
    /// `#rheo-page-init("<handle>")` — the per-document init hook defined in
    /// `typ/rheo.typ`: it publishes the page handle to `state` and, for per-page
    /// (html/epub) output, resets the footnote counter so each page numbers its
    /// footnotes from 1.
    PageInit { handle: String },
    /// A cross-vertebra handle anchor: a labeled, hidden `rheo-handle` `#figure`
    /// so `@label` / `#link(<label>)` resolve across the bundle. The figure's
    /// body is a live `#context` query of the owning vertebra's
    /// [`TypstStmt::MetadataBeacon`] (`rheo-meta:<handle>`), so `@handle`
    /// display text tracks that vertebra's *current* `document.title` rather
    /// than a title baked in at bundle-synthesis time. `handle` is the
    /// vertebra's canonical handle (the beacon query target, shared by every
    /// anchor belonging to that vertebra); `fallback_title` is the
    /// path-derived title used only when no beacon is found (combined PDF
    /// layouts, which emit no beacons).
    HandleAnchor {
        label: String,
        handle: String,
        fallback_title: String,
    },
    /// `#include "<path>"`.
    Include { path: String },
    /// A `#document("<output_path>", format: "<format>", title: [<title>])[ … ]`
    /// block wrapping a body of statements, each rendered indented.
    Document {
        output_path: String,
        format: String,
        title: String,
        body: Vec<TypstStmt>,
    },
}

impl fmt::Display for TypstStmt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypstStmt::Raw(s) => f.write_str(s),
            TypstStmt::Let { name, value } => {
                write!(f, "#let {} = {}", name, value.serialize())
            }
            TypstStmt::ContextBinding { handle } => write!(
                f,
                "#let rheo-context() = (handle: {}, metadata-of: rheo-metadata, ..sys.inputs.rheo-context)",
                TypstLiteral::str(handle.as_str()).serialize()
            ),
            TypstStmt::MetadataHelper => write!(
                f,
                "#let rheo-metadata(handle) = {{\n\
                 \x20 let found = query(label(\"rheo-meta:\" + handle))\n\
                 \x20 if found.len() == 0 {{ return (:) }}\n\
                 \x20 let out = (:)\n\
                 \x20 for (k, v) in found.first().value {{\n\
                 \x20   if k == \"handle\" or v == none or v == auto {{ continue }}\n\
                 \x20   if type(v) == array and v.len() == 0 {{ continue }}\n\
                 \x20   out.insert(k, v)\n\
                 \x20 }}\n\
                 \x20 out\n\
                 }}"
            ),
            TypstStmt::MetadataBeacon { handle } => write!(
                f,
                "#context [#metadata((handle: {handle_lit}, title: document.title, author: document.author, description: document.description, keywords: document.keywords, date: document.date)) <rheo-meta:{handle}>]",
                handle_lit = TypstLiteral::str(handle.as_str()).serialize(),
            ),
            TypstStmt::StateUpdate { key, value } => write!(
                f,
                "#state({}).update({})",
                TypstLiteral::str(key.as_str()).serialize(),
                value.serialize()
            ),
            TypstStmt::PageInit { handle } => write!(
                f,
                "#rheo-page-init({})",
                TypstLiteral::str(handle.as_str()).serialize()
            ),
            TypstStmt::HandleAnchor {
                label,
                handle,
                fallback_title,
            } => write!(
                f,
                "#figure([#context {{\n\
                 \x20 let m = query(label(\"rheo-meta:\" + {handle_lit}))\n\
                 \x20 if m.len() > 0 and m.first().value.title != none {{ m.first().value.title }} else [{fallback}]\n\
                 }}], kind: \"rheo-handle\", supplement: none) <{label}>",
                handle_lit = TypstLiteral::str(handle.as_str()).serialize(),
                fallback = escape_typst_content(fallback_title),
            ),
            TypstStmt::Include { path } => write!(f, "#include \"{}\"", path),
            TypstStmt::Document {
                output_path,
                format,
                title,
                body,
            } => {
                writeln!(
                    f,
                    "#document(\"{}\", format: \"{}\", title: [{}])[",
                    output_path,
                    format,
                    escape_typst_content(title)
                )?;
                for stmt in body {
                    writeln!(f, "  {stmt}")?;
                }
                write!(f, "]")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn let_renders_binding_with_serialized_value() {
        let stmt = TypstStmt::Let {
            name: "rheo-context".into(),
            value: TypstLiteral::Dict(vec![("handle".into(), TypstLiteral::str("intro"))]),
        };
        assert_eq!(stmt.to_string(), "#let rheo-context = (handle: \"intro\")");
    }

    #[test]
    fn context_binding_composes_handle_with_sys_inputs() {
        let stmt = TypstStmt::ContextBinding {
            handle: "chapters:ch1".into(),
        };
        assert_eq!(
            stmt.to_string(),
            "#let rheo-context() = (handle: \"chapters:ch1\", metadata-of: rheo-metadata, ..sys.inputs.rheo-context)"
        );
    }

    #[test]
    fn metadata_helper_renders_fixed_query_function() {
        let stmt = TypstStmt::MetadataHelper;
        assert_eq!(
            stmt.to_string(),
            "#let rheo-metadata(handle) = {\n\
             \x20 let found = query(label(\"rheo-meta:\" + handle))\n\
             \x20 if found.len() == 0 { return (:) }\n\
             \x20 let out = (:)\n\
             \x20 for (k, v) in found.first().value {\n\
             \x20   if k == \"handle\" or v == none or v == auto { continue }\n\
             \x20   if type(v) == array and v.len() == 0 { continue }\n\
             \x20   out.insert(k, v)\n\
             \x20 }\n\
             \x20 out\n\
             }"
        );
    }

    #[test]
    fn metadata_beacon_quotes_handle_in_value_and_label() {
        let stmt = TypstStmt::MetadataBeacon {
            handle: "chapters:intro".into(),
        };
        assert_eq!(
            stmt.to_string(),
            "#context [#metadata((handle: \"chapters:intro\", title: document.title, author: document.author, description: document.description, keywords: document.keywords, date: document.date)) <rheo-meta:chapters:intro>]"
        );
    }

    #[test]
    fn state_update_quotes_key_and_value() {
        let stmt = TypstStmt::StateUpdate {
            key: "rheo-handle".into(),
            value: TypstLiteral::str("chapters:ch1"),
        };
        assert_eq!(
            stmt.to_string(),
            "#state(\"rheo-handle\").update(\"chapters:ch1\")"
        );
    }

    #[test]
    fn page_init_quotes_handle() {
        let stmt = TypstStmt::PageInit {
            handle: "chapters:ch1".into(),
        };
        assert_eq!(stmt.to_string(), "#rheo-page-init(\"chapters:ch1\")");
    }

    #[test]
    fn handle_anchor_renders_live_beacon_query_with_escaped_fallback() {
        let stmt = TypstStmt::HandleAnchor {
            label: "intro".into(),
            handle: "intro".into(),
            fallback_title: "Intro".into(),
        };
        assert_eq!(
            stmt.to_string(),
            "#figure([#context {\n\
             \x20 let m = query(label(\"rheo-meta:\" + \"intro\"))\n\
             \x20 if m.len() > 0 and m.first().value.title != none { m.first().value.title } else [Intro]\n\
             }], kind: \"rheo-handle\", supplement: none) <intro>"
        );
    }

    #[test]
    fn handle_anchor_uses_vertebra_handle_for_query_even_for_escape_alias_label() {
        // An escape-alias anchor (`<handle.typ>`) has a different `label` than
        // the vertebra's canonical `handle`, but must query the SAME beacon.
        let stmt = TypstStmt::HandleAnchor {
            label: "intro.typ".into(),
            handle: "intro".into(),
            fallback_title: "Intro".into(),
        };
        assert_eq!(
            stmt.to_string(),
            "#figure([#context {\n\
             \x20 let m = query(label(\"rheo-meta:\" + \"intro\"))\n\
             \x20 if m.len() > 0 and m.first().value.title != none { m.first().value.title } else [Intro]\n\
             }], kind: \"rheo-handle\", supplement: none) <intro.typ>"
        );
    }

    #[test]
    fn document_wraps_indented_body() {
        let doc = TypstStmt::Document {
            output_path: "chapters/ch1.html".into(),
            format: "html".into(),
            title: "Chapter 1".into(),
            body: vec![
                TypstStmt::StateUpdate {
                    key: "rheo-handle".into(),
                    value: TypstLiteral::str("chapters:ch1"),
                },
                TypstStmt::Include {
                    path: "content/chapters/ch1.typ".into(),
                },
            ],
        };
        assert_eq!(
            doc.to_string(),
            "#document(\"chapters/ch1.html\", format: \"html\", title: [Chapter 1])[\n  \
             #state(\"rheo-handle\").update(\"chapters:ch1\")\n  \
             #include \"content/chapters/ch1.typ\"\n]"
        );
    }
}

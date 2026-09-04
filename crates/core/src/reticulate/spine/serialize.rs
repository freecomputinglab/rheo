use super::VirtualSpine;
use super::tree::SpineNode;
use crate::synth::typst_literal::TypstLiteral;
use std::collections::HashMap;

/// The per-format values that ride on `sys.inputs.rheo-context` alongside the
/// spine itself. See [`VirtualSpine::global_context`].
pub struct FormatContext<'a> {
    /// The rheo output-format name (`"html"`/`"epub"`). `None` for PDF, which
    /// sets no rheo target and falls back to Typst's native `target()`.
    pub target: Option<&'a str>,
    /// The output file extension (`"html"`/`"xhtml"`) — what `typ/rheo.typ`
    /// reads to build cross-vertebra link hrefs. Gated exactly like `target`.
    pub ext: Option<&'a str>,
    /// The resolved per-format `reset-footnotes` toggle. Unlike `target`/`ext`
    /// this is always present; `typ/rheo.typ` ANDs it with the per-page `ext`
    /// gate, so it only ever takes effect for HTML/EPUB.
    pub reset_footnotes: bool,
    /// Rust's own post-compile `DocumentInfo` title for a vertebra whose
    /// beacon read it wrong — a title set inside a bounded code block, see
    /// `docs/limitations.md`. Empty on the ordinary single pass; populated only
    /// by the gated second pass of `Build::compile_bundle_once`.
    pub title_overrides: &'a HashMap<String, String>,
}

impl VirtualSpine {
    /// The file-independent `rheo-context` data exposed via `sys.inputs`.
    ///
    /// `sys.inputs` is global to the whole bundle compile, so it carries only
    /// the parts identical across vertebrae: `spine`/`spine-flat`, the
    /// compiling rheo's own `rheo-version` (a package reads it to enforce a
    /// minimum rheo, treating its absence as "older than the release that
    /// added it"), and the per-format values in `format` — see
    /// [`FormatContext`] for each. Packages read `sys.inputs.rheo-context` to
    /// detect a rheo build without referencing the per-file `rheo-context()`,
    /// which additionally carries that file's `handle`.
    ///
    /// `title-overrides` is serialized as an array of `(handle, title)` dicts
    /// rather than a dict keyed by handle, since a handle like
    /// `"chapters:intro"` is not a valid Typst identifier — the same reason
    /// `spine-flat` is an array of handle-bearing dicts.
    pub fn global_context(&self, format: FormatContext<'_>) -> TypstLiteral {
        let FormatContext {
            target,
            ext,
            reset_footnotes,
            title_overrides,
        } = format;
        let mut fields = vec![
            ("spine".to_string(), self.spine_tree()),
            ("spine-flat".to_string(), self.spine_flat()),
            (
                "rheo-version".to_string(),
                TypstLiteral::str(env!("CARGO_PKG_VERSION")),
            ),
        ];
        if let Some(t) = target {
            fields.push(("target".to_string(), TypstLiteral::str(t)));
        }
        if let Some(e) = ext {
            fields.push(("ext".to_string(), TypstLiteral::str(e)));
        }
        fields.push((
            "reset-footnotes".to_string(),
            TypstLiteral::bool(reset_footnotes),
        ));
        fields.push((
            "title-overrides".to_string(),
            TypstLiteral::Array(
                title_overrides
                    .iter()
                    .map(|(handle, title)| {
                        TypstLiteral::Dict(vec![
                            ("handle".to_string(), TypstLiteral::str(handle.as_str())),
                            ("title".to_string(), TypstLiteral::str(title.as_str())),
                        ])
                    })
                    .collect(),
            ),
        ));
        TypstLiteral::Dict(fields)
    }

    /// The structured spine tree as a [`TypstLiteral`] array of recursive node
    /// dicts. See [`Self::node_literal`] for the node key set.
    fn spine_tree(&self) -> TypstLiteral {
        TypstLiteral::Array(self.tree.iter().map(|n| self.node_literal(n)).collect())
    }

    /// Serialize one [`SpineNode`] (and its descendants) to its `spine` dict
    /// shape: `title`/`handle`/`path`/`children`. Per-vertebra document
    /// metadata is not part of this shape — read it live via
    /// `rheo-context().metadata-of` (see [`TypstStmt::MetadataHelper`]).
    fn node_literal(&self, node: &SpineNode) -> TypstLiteral {
        node.fold(&mut |n, children| {
            let (handle, path, title) = match n.vertebra().and_then(|&i| self.vertebrae.get(i)) {
                Some(v) => (
                    TypstLiteral::str(v.handle.as_str()),
                    TypstLiteral::str(v.rel_path.as_str()),
                    TypstLiteral::str(v.title.as_str()),
                ),
                None => (
                    TypstLiteral::None,
                    TypstLiteral::None,
                    TypstLiteral::str(n.title().unwrap_or(n.segment.as_str())),
                ),
            };
            TypstLiteral::Dict(vec![
                ("title".to_string(), title),
                ("handle".to_string(), handle),
                ("path".to_string(), path),
                ("children".to_string(), TypstLiteral::Array(children)),
            ])
        })
    }

    /// The flat spine as a [`TypstLiteral`] array-of-dictionaries, in the same
    /// pre-order as [`Self::flat_vertebrae`]: one entry per clickable vertebra
    /// (group nodes excluded) with `handle`, `path`, and `title`.
    fn spine_flat(&self) -> TypstLiteral {
        TypstLiteral::Array(
            self.flat_vertebrae()
                .into_iter()
                .map(|v| {
                    TypstLiteral::Dict(vec![
                        ("handle".to_string(), TypstLiteral::str(v.handle.as_str())),
                        ("path".to_string(), TypstLiteral::str(v.rel_path.as_str())),
                        ("title".to_string(), TypstLiteral::str(v.title.as_str())),
                    ])
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reticulate::spine::{SpineLayout, SpineScan};
    use std::fs;
    use tempfile::TempDir;

    /// A per-page (HTML) format context with footnote reset on.
    fn html_context(title_overrides: &HashMap<String, String>) -> FormatContext<'_> {
        FormatContext {
            target: Some("html"),
            ext: Some("html"),
            reset_footnotes: true,
            title_overrides,
        }
    }

    /// A combined-PDF format context: no target, no ext.
    fn pdf_context(title_overrides: &HashMap<String, String>) -> FormatContext<'_> {
        FormatContext {
            target: None,
            ext: None,
            reset_footnotes: true,
            title_overrides,
        }
    }

    #[test]
    fn spine_tree_nests_group_nodes_with_none_handle_and_path() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        let chapters = content.join("chapters");
        fs::create_dir_all(&chapters).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();
        fs::write(chapters.join("one.typ"), "= One\n").unwrap();

        let scan = SpineScan::run(&content, &[]).unwrap();
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(scan, root, layout).unwrap();

        let tree = spine.spine_tree().serialize();
        // Root leaf carries its own handle/path/title.
        assert!(tree.contains("handle: \"intro\""));
        assert!(tree.contains("path: \"content/intro.typ\""));
        // The `chapters` directory has no landing page: a group node with
        // handle/path `none` and its own title, nesting `one` as a child.
        assert!(tree.contains("handle: none"));
        assert!(tree.contains("path: none"));
        assert!(tree.contains("title: \"Chapters\""));
        assert!(tree.contains("children:"));
        assert!(tree.contains("handle: \"chapters:one\""));

        // spine-flat only lists clickable vertebrae, in pre-order.
        let flat = spine.spine_flat().serialize();
        assert!(flat.contains("handle: \"intro\""));
        assert!(flat.contains("handle: \"chapters:one\""));
        assert!(!flat.contains("title: \"Chapters\""));
    }

    #[test]
    fn spine_no_longer_exposes_a_metadata_key_on_entries() {
        // Neither the spine tree nor spine-flat entries carry a `metadata`
        // key — `Vertebra.title` is purely path-derived, so this vertebra's
        // spine-entry title is "Post" (from the filename), not the
        // `#set document(title: ...)` value. Read live document metadata via
        // `rheo-context().metadata-of` instead.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // A post whose `#set document(...)` carries keywords (tags) and an author.
        fs::write(
            content.join("post.typ"),
            "#set document(title: [My Post], keywords: (\"DiH\",), author: \"Jane\")\n= Body\n",
        )
        .unwrap();
        // A page with no `#set document(...)`.
        fs::write(content.join("bare.typ"), "= Bare\n").unwrap();

        let scan = SpineScan::run(&content, &[]).unwrap();
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(scan, root, layout).unwrap();

        for serialized in [
            spine.spine_tree().serialize(),
            spine.spine_flat().serialize(),
        ] {
            assert!(
                !serialized.contains("metadata:"),
                "metadata key should no longer be serialized: {serialized}"
            );
            // The other spine entry fields remain.
            assert!(serialized.contains("title: \"Post\""));
            assert!(serialized.contains("handle: \"post\""));
        }
    }

    #[test]
    fn rheo_context_target_and_ext_present_when_some_absent_when_none() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        // target/ext live on the global context (sys.inputs); the per-file
        // prelude only spreads them in, so they are asserted on global_context.
        let global_html = spine
            .global_context(html_context(&HashMap::new()))
            .serialize();
        assert!(global_html.contains("target: \"html\""));
        assert!(global_html.contains("ext: \"html\""));
        // The resolved reset-footnotes toggle is always present (unlike target/ext).
        assert!(global_html.contains("reset-footnotes: true"));

        // Epub keeps `target` "epub" but `ext` "xhtml"; a false toggle is threaded through.
        let global_epub = spine
            .global_context(FormatContext {
                target: Some("epub"),
                ext: Some("xhtml"),
                reset_footnotes: false,
                title_overrides: &HashMap::new(),
            })
            .serialize();
        assert!(global_epub.contains("target: \"epub\""));
        assert!(global_epub.contains("ext: \"xhtml\""));
        assert!(global_epub.contains("reset-footnotes: false"));

        // None (PDF) -> no `target` or `ext` field, but reset-footnotes is still present.
        let global_without = spine
            .global_context(pdf_context(&HashMap::new()))
            .serialize();
        assert!(!global_without.contains("target:"));
        assert!(!global_without.contains("ext:"));
        assert!(global_without.contains("reset-footnotes: true"));
    }

    #[test]
    fn global_context_carries_rheo_version() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        let expected = format!("rheo-version: \"{}\"", env!("CARGO_PKG_VERSION"));
        assert!(
            spine
                .global_context(pdf_context(&HashMap::new()))
                .serialize()
                .contains(&expected)
        );
    }

    #[test]
    fn global_context_title_overrides_serializes_as_handle_title_array() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(SpineScan::flat(&files, &content), root, layout).unwrap();

        // Empty by default (ordinary single pass): still a present, empty array.
        let empty = spine
            .global_context(pdf_context(&HashMap::new()))
            .serialize();
        assert!(empty.contains("title-overrides: ()"));

        // A handle like "chapters:intro" is not a valid Typst identifier, so
        // overrides must be an array of dicts, not a dict keyed by handle.
        let overrides = HashMap::from([("chapters:intro".to_string(), "Real Title".to_string())]);
        let with_override = spine.global_context(pdf_context(&overrides)).serialize();
        assert!(
            with_override.contains(
                "title-overrides: ((handle: \"chapters:intro\", title: \"Real Title\"),)"
            )
        );
    }
}

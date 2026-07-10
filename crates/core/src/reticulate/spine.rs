use crate::parser;
use crate::parser::{DocumentDate, LabelSites, RheoValue};
use crate::plugins::SpineOptions;
use crate::reticulate::bundle_source::BundleSource;
use crate::util::path::{sanitize_handle_segment, to_forward_slash};
use crate::util::pdf::DocumentTitle;
use crate::{Result, RheoError, TYP_EXT};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use typst::syntax::Source;
use walkdir::WalkDir;

fn collect_typst_files(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| Some(entry.ok()?.path().to_path_buf()))
        .filter(|entry| {
            entry
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == &TYP_EXT[1..])
                .unwrap_or(false)
        })
        .collect()
}

fn collect_all_typst_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut typst_files = collect_typst_files(root);

    if typst_files.is_empty() {
        return Err(RheoError::project_config("need at least one .typ file"));
    }

    typst_files.sort();
    Ok(typst_files)
}

/// Generates a spine (ordered list of .typ files) based on configuration.
impl SpineOptions {
    /// Resolve vertebrae patterns into an ordered list of .typ files.
    ///
    /// If no vertebrae are configured, discovers all .typ files under `root`.
    pub fn generate(&self, root: &Path) -> Result<Vec<PathBuf>> {
        if self.vertebrae.is_empty() {
            return collect_all_typst_files(root);
        }

        let mut typst_files = Vec::new();
        for pattern in &self.vertebrae {
            let glob_pattern = root.join(pattern).display().to_string();
            let glob = glob::glob(&glob_pattern).map_err(|e| {
                RheoError::project_config(format!("invalid glob pattern '{}': {}", pattern, e))
            })?;

            let mut glob_files: Vec<PathBuf> = glob
                .filter_map(|entry| entry.ok())
                .filter(|path| path.is_file())
                .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("typ"))
                .collect();

            // Sort by full path (lexicographic) for consistent ordering
            glob_files.sort();
            typst_files.extend(glob_files);
        }

        if typst_files.is_empty() {
            return Err(RheoError::project_config("spine matched no .typ files"));
        }

        Ok(typst_files)
    }
}

// ── Bundle spine: VirtualSpine, Vertebra, SpineLayout ────────────────────────

/// How a spine is compiled into output files under the bundle path.
pub enum SpineLayout {
    /// One output file per vertebra (e.g. HTML: "intro.html", "closing.html").
    OnePerVertebra { ext: String, format: String },
    /// All vertebrae in one combined output (e.g. PDF: "doc.pdf").
    SingleCombined { output_name: String, format: String },
}

/// Resolved metadata for one vertebra in a bundle compile.
pub struct Vertebra {
    /// Path relative to the project root, forward-slash separated (for `#include`).
    pub rel_path: String,
    /// Output path key in VirtualFs (e.g. "intro.html").
    pub output_path: String,
    /// Primary synthesized cross-vertebra handle label (e.g. "intro" or "chapters:intro").
    pub handle: String,
    /// Additional handle aliases; always includes the `<stem.typ>` escape form.
    pub extra_handles: Vec<String>,
    /// Whether the canonical `<handle>` label should be emitted as a bundle anchor.
    /// False when a user-authored label already occupies the canonical name.
    pub emit_handle: bool,
    /// Document title for `#document title:` and `@handle` display text.
    pub title: String,
    /// Parsed `#set document(date: datetime(...))` timestamp, if present.
    pub date: Option<DocumentDate>,
    /// Harvested `rheo-*` variables from this vertebra's source file.
    pub vars: std::collections::HashMap<String, RheoValue>,
    /// The vertebra's raw source text, retained for the Mould stage.
    pub source: String,
    /// Label definition and reference sites extracted from `source`; the Mould
    /// stage turns these into rewrites.
    pub sites: LabelSites,
}

impl Vertebra {
    /// Return `true` if this vertebra's output path collides with `other`.
    pub fn collides_with(&self, other: &Vertebra) -> bool {
        self.output_path == other.output_path
    }
}

/// A resolved spine ready for bundle compilation.
///
/// Constructed via `VirtualSpine::build`; call `source()` to get the synthesized
/// Typst source that drives `RheoWorld::compile_bundle`.
pub struct VirtualSpine {
    pub vertebrae: Vec<Vertebra>,
    pub layout: SpineLayout,
}

impl VirtualSpine {
    /// Resolve a list of spine files into a `VirtualSpine` with computed handles,
    /// output paths, and titles.
    ///
    /// `content_dir` is the project content root; stems are computed relative to it
    /// so `content/chapters/intro.typ` yields handle `intro` (or `chapters:intro`
    /// on a cross-directory stem collision). Pass `project_root` for `#include` paths.
    pub fn build(
        files: &[PathBuf],
        content_dir: &Path,
        project_root: &Path,
        layout: SpineLayout,
        prefix_labels: bool,
    ) -> Result<Self> {
        // Stem relative to content_dir (no extension, forward-slash).
        let rel_stems: Vec<String> = files
            .iter()
            .map(|f| to_forward_slash(&f.strip_prefix(content_dir).unwrap_or(f).with_extension("")))
            .collect();

        // First pass: parse each file, compute handles, collect user labels.
        struct FileInfo {
            file: PathBuf,
            handle: String,
            escape: String,
            output_path: String,
            rel_path: String,
            title: String,
            date: Option<DocumentDate>,
            vars: HashMap<String, RheoValue>,
            source: String,
            sites: LabelSites,
        }

        let file_infos: Result<Vec<FileInfo>> = files
            .iter()
            .zip(rel_stems.iter())
            .map(|(file, rel_stem)| {
                let segments: Vec<&str> = rel_stem.split('/').collect();

                // Canonical handle: bare stem for root-level files, path-prefixed
                // with ':' separator for nested files ("a/notes" → "a:notes").
                let handle = if segments.len() > 1 {
                    segments
                        .iter()
                        .map(|s| sanitize_handle_segment(s))
                        .collect::<Vec<_>>()
                        .join(":")
                } else {
                    sanitize_handle_segment(segments[0])
                };

                let escape = format!("{handle}.typ");

                let output_path = match &layout {
                    SpineLayout::OnePerVertebra { ext, .. } => format!("{handle}.{ext}"),
                    SpineLayout::SingleCombined { output_name, .. } => output_name.clone(),
                };

                let source = fs::read_to_string(file).unwrap_or_default();
                let stem = file
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let title = DocumentTitle::from_source(&source, &stem).extract();
                let rel_path = to_forward_slash(file.strip_prefix(project_root).unwrap_or(file));

                let source_obj = Source::detached(&source);
                let extracted = parser::extract_nodes(&source_obj);
                let date = extracted.document_date;
                let sites = extracted.labels;
                let mut vars = HashMap::new();
                for v in extracted.rheo_vars {
                    match v.value {
                        Some(value) => {
                            vars.insert(v.key, value);
                        }
                        None => {
                            return Err(RheoError::invalid_data(format!(
                                "{}:{}: rheo-{} must be a string or boolean",
                                file.display(),
                                v.line,
                                v.key
                            )));
                        }
                    }
                }

                Ok(FileInfo {
                    file: file.clone(),
                    handle,
                    escape,
                    output_path,
                    rel_path,
                    title,
                    date,
                    vars,
                    source,
                    sites,
                })
            })
            .collect();
        let file_infos = file_infos?;

        // Union of all user-authored labels across the spine, as they will land
        // in the bundle. When `prefix_labels` is on, the Mould stage namespaces
        // every definition to `handle:name` (see `LabelRewrite`), so the checks
        // below must compare against the prefixed names — the same rule Mould
        // uses. A consequence: a prefixed label can never equal the bare
        // `handle` or the `handle.typ` escape (those synthesized anchors are not
        // prefixed), so section labels no longer suppress a handle anchor or
        // collide with an escape. Whole-page `@handle` / `@handle.typ` still
        // resolve via the synthesized anchors.
        let all_user_labels: HashSet<String> = file_infos
            .iter()
            .flat_map(|fi| {
                fi.sites.definitions.iter().map(move |d| {
                    if prefix_labels {
                        format!("{}:{}", fi.handle, d.name)
                    } else {
                        d.name.clone()
                    }
                })
            })
            .collect();

        // Second pass: assign emit_handle and check escape uniqueness.
        let mut seen_canonicals: HashSet<String> = HashSet::new();
        let mut seen_escapes: HashSet<String> = HashSet::new();

        let vertebrae: Result<Vec<Vertebra>> = file_infos
            .into_iter()
            .map(|fi| {
                // Canonical: skip if claimed by user or already emitted.
                let emit_handle =
                    !all_user_labels.contains(&fi.handle) && !seen_canonicals.contains(&fi.handle);
                seen_canonicals.insert(fi.handle.clone());

                // Escape: must be unique — error on collision.
                if all_user_labels.contains(&fi.escape) || seen_escapes.contains(&fi.escape) {
                    return Err(RheoError::invalid_data(format!(
                        "{}: escape label <{}> collides with another label in the project",
                        fi.file.display(),
                        fi.escape
                    )));
                }
                seen_escapes.insert(fi.escape.clone());

                Ok(Vertebra {
                    rel_path: fi.rel_path,
                    output_path: fi.output_path,
                    handle: fi.handle,
                    extra_handles: vec![fi.escape],
                    emit_handle,
                    title: fi.title,
                    date: fi.date,
                    vars: fi.vars,
                    source: fi.source,
                    sites: fi.sites,
                })
            })
            .collect();

        Ok(Self {
            vertebrae: vertebrae?,
            layout,
        })
    }

    /// Validate that no two vertebrae produce the same output path.
    ///
    /// Skipped for `SingleCombined` layouts where all vertebrae intentionally share
    /// the same output path (e.g. every vertebra produces "book.pdf").
    pub fn check_output_collisions(&self) -> Result<()> {
        if !matches!(self.layout, SpineLayout::OnePerVertebra { .. }) {
            return Ok(());
        }
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for (i, v) in self.vertebrae.iter().enumerate() {
            if let Some(prev) = seen.insert(v.output_path.as_str(), i) {
                return Err(RheoError::project_config(format!(
                    "spine output path collision: '{}' produced by vertebra {} and {}",
                    v.output_path, prev, i
                )));
            }
        }
        Ok(())
    }

    /// Synthesize the virtual main Typst source that drives `RheoWorld::compile_bundle`.
    ///
    /// For `OnePerVertebra` each vertebra becomes a `#document(output-path)[...]` containing
    /// a labeled `#figure` handle anchor followed by a real `#include`.
    /// For `SingleCombined` all vertebrae are wrapped in one `#document`, each with its
    /// own handle anchors emitted immediately before its `#include` so cross-references
    /// resolve to the correct location within the combined output.
    ///
    /// The handle anchor uses `#figure` rather than `#metadata` or a bare label.
    /// `#metadata(none) <label>` fails at compile time ("cannot reference metadata");
    /// `<label>` after a `#document` block labels the document element itself, which
    /// Typst also refuses to reference ("cannot reference document"). A labeled
    /// `#figure([title], kind: "rheo-handle", …)` is the only mechanism that allows
    /// cross-document `@handle` resolution. The corresponding `#show figure.where(kind:
    /// "rheo-handle"): none` in rheo.typ suppresses its rendering.
    pub fn source(&self) -> String {
        self.bundle_source().to_string()
    }

    /// Build the structured `BundleSource` representation of this spine.
    pub fn bundle_source(&self) -> BundleSource {
        use crate::reticulate::bundle_source::{BundleAnchor, BundleDocument, BundleSegment};

        // A vertebra's handle anchors: the canonical `<handle>` (when emitted) plus
        // the `<handle.typ>` escape aliases. Emitted before the vertebra's include so
        // cross-references resolve to the right location in the output.
        let segment_for = |v: &Vertebra| BundleSegment {
            anchors: v
                .emit_handle
                .then_some(&v.handle)
                .into_iter()
                .chain(v.extra_handles.iter())
                .map(|label| BundleAnchor {
                    label: label.clone(),
                    title: v.title.clone(),
                })
                .collect(),
            include: v.rel_path.clone(),
        };

        let documents = match &self.layout {
            SpineLayout::OnePerVertebra { format, .. } => self
                .vertebrae
                .iter()
                .map(|v| BundleDocument {
                    output_path: v.output_path.clone(),
                    format: format.clone(),
                    title: v.title.clone(),
                    segments: vec![segment_for(v)],
                })
                .collect(),
            SpineLayout::SingleCombined {
                output_name,
                format,
            } => {
                let title = self
                    .vertebrae
                    .first()
                    .map(|v| v.title.as_str())
                    .unwrap_or("Document")
                    .to_string();
                vec![BundleDocument {
                    output_path: output_name.clone(),
                    format: format.clone(),
                    title,
                    segments: self.vertebrae.iter().map(segment_for).collect(),
                }]
            }
        };

        BundleSource { documents }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_dir_with_files(files: &[&str]) -> TempDir {
        let temp = TempDir::new().unwrap();
        for file in files {
            let path = temp.path().join(file);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&path, "").unwrap();
        }
        temp
    }

    fn spine_with_vertebrae(vertebrae: Vec<String>) -> SpineOptions {
        SpineOptions {
            title: Some("Test".to_string()),
            vertebrae,
        }
    }

    #[test]
    fn test_generate_with_vertebrae() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ", "c.typ"]);
        let spine = spine_with_vertebrae(vec!["*.typ".to_string()]);
        let result = spine.generate(temp.path());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 3);
    }

    #[test]
    fn test_generate_ordered_patterns() {
        let temp = create_test_dir_with_files(&[
            "cover.typ",
            "chapters/ch1.typ",
            "chapters/ch2.typ",
            "appendix.typ",
        ]);
        let spine = SpineOptions {
            title: Some("Book".to_string()),
            vertebrae: vec![
                "cover.typ".to_string(),
                "chapters/*.typ".to_string(),
                "appendix.typ".to_string(),
            ],
        };
        let result = spine.generate(temp.path());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 4);
        assert_eq!(files[0].file_name().unwrap(), "cover.typ");
        assert!(
            files[1]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("ch")
        );
        assert!(
            files[2]
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("ch")
        );
        assert_eq!(files[3].file_name().unwrap(), "appendix.typ");
    }

    #[test]
    fn test_generate_no_matches_error() {
        let temp = create_test_dir_with_files(&["readme.md"]);
        let spine = spine_with_vertebrae(vec!["*.typ".to_string()]);
        let result = spine.generate(temp.path());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("spine matched no .typ files")
        );
    }

    #[test]
    fn test_generate_empty_pattern_single_file() {
        let temp = create_test_dir_with_files(&["single.typ"]);
        let spine = spine_with_vertebrae(vec![]);
        let result = spine.generate(temp.path());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_generate_empty_pattern_multiple_files_returns_all() {
        let temp = create_test_dir_with_files(&["a.typ", "b.typ"]);
        let spine = spine_with_vertebrae(vec![]);
        let result = spine.generate(temp.path());
        assert!(result.is_ok());
        let files = result.unwrap();
        assert_eq!(files.len(), 2);
    }

    // ── VirtualSpine tests ──────────────────────────────────────────────────

    #[test]
    fn unique_stems_get_bare_handle() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro\n").unwrap();
        fs::write(content.join("closing.typ"), "= Closing\n").unwrap();

        let files = vec![content.join("intro.typ"), content.join("closing.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(&files, &content, root, layout, true).unwrap();

        assert_eq!(spine.vertebrae[0].handle, "intro");
        assert_eq!(spine.vertebrae[0].extra_handles, vec!["intro.typ"]);
        assert_eq!(spine.vertebrae[0].output_path, "intro.html");
        assert_eq!(spine.vertebrae[1].handle, "closing");
        assert_eq!(spine.vertebrae[1].output_path, "closing.html");
    }

    #[test]
    fn vertebra_retains_source_and_label_sites() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro <etal>\n\nSee @etal.\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(&files, &content, root, layout, true).unwrap();
        let v = &spine.vertebrae[0];

        // The raw source is retained for the Mould stage.
        assert!(v.source.contains("<etal>"));
        // The definition and the reference are both captured, with byte ranges.
        assert_eq!(
            v.sites
                .definitions
                .iter()
                .map(|d| d.name.as_str())
                .collect::<Vec<_>>(),
            vec!["etal"]
        );
        assert_eq!(
            v.sites
                .references
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            vec!["etal"]
        );
        let def = &v.sites.definitions[0];
        assert_eq!(&v.source[def.range.clone()], "<etal>");
    }

    #[test]
    fn mould_prefixes_labels_and_gates_on_flag() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(content.join("intro.typ"), "= Intro <etal>\n\nSee @etal.\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(&files, &content, root, layout, true).unwrap();

        // main is the synthesized bundle source in both cases.
        let moulded = spine.mould(true);
        assert_eq!(moulded.main, spine.source());
        // With prefixing on, the vertebra's body is rewritten under its handle.
        let body = &moulded.sources["content/intro.typ"];
        assert!(body.contains("<intro:etal>"));
        assert!(body.contains("@intro:etal"));

        // With prefixing off, nothing is rewritten (no overlay entry).
        assert!(spine.mould(false).sources.is_empty());
    }

    #[test]
    fn included_non_vertebra_partial_is_not_moulded() {
        // A vertebra `#include`s a partial that is NOT itself a spine vertebra.
        // Only spine vertebrae are moulded, so the partial is absent from the
        // overlay — Typst reads it from disk and its labels stay unprefixed.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        fs::write(
            content.join("intro.typ"),
            "= Intro <top>\n\n#include \"_shared.typ\"\n",
        )
        .unwrap();
        // A shared partial defining its own label — not listed as a vertebra.
        fs::write(content.join("_shared.typ"), "== Shared <shared>\n").unwrap();

        // Only intro.typ is a spine vertebra; _shared.typ is a transitive include.
        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(&files, &content, root, layout, true).unwrap();
        let moulded = spine.mould(true);

        // Only the vertebra is in the overlay; the partial is not (disk-served).
        assert_eq!(moulded.sources.len(), 1);
        assert!(moulded.sources.contains_key("content/intro.typ"));
        assert!(!moulded.sources.contains_key("content/_shared.typ"));

        // The vertebra's own label is prefixed; the `#include` is untouched, so the
        // partial's `<shared>` reaches the bundle from disk, unprefixed.
        let body = &moulded.sources["content/intro.typ"];
        assert!(body.contains("<intro:top>"));
        assert!(body.contains("#include \"_shared.typ\""));
    }

    #[test]
    fn nested_files_get_colon_qualified_handle() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        let a = content.join("a");
        let b = content.join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("notes.typ"), "").unwrap();
        fs::write(b.join("notes.typ"), "").unwrap();

        let files = vec![a.join("notes.typ"), b.join("notes.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(&files, &content, root, layout, true).unwrap();

        assert_eq!(spine.vertebrae[0].handle, "a:notes");
        assert_eq!(spine.vertebrae[1].handle, "b:notes");
        assert!(
            spine.vertebrae[0]
                .extra_handles
                .contains(&"a:notes.typ".to_string())
        );
        assert_ne!(
            spine.vertebrae[0].output_path,
            spine.vertebrae[1].output_path
        );
    }

    #[test]
    fn virtual_main_html_shape() {
        let v = Vertebra {
            rel_path: "content/intro.typ".into(),
            output_path: "intro.html".into(),
            handle: "intro".into(),
            extra_handles: vec!["intro.typ".into()],
            emit_handle: true,
            title: "Introduction".into(),
            date: None,
            vars: HashMap::new(),
            source: String::new(),
            sites: LabelSites::default(),
        };
        let spine = VirtualSpine {
            vertebrae: vec![v],
            layout: SpineLayout::OnePerVertebra {
                ext: "html".into(),
                format: "html".into(),
            },
        };
        let src = spine.source();
        assert!(src.contains("#document(\"intro.html\", format: \"html\""));
        assert!(src.contains("rheo-handle"));
        assert!(src.contains("[Introduction]"));
        assert!(src.contains("<intro>"));
        assert!(src.contains("<intro.typ>"));
        assert!(src.contains("#include \"content/intro.typ\""));
    }

    #[test]
    fn virtual_main_pdf_shape() {
        let spine = VirtualSpine {
            vertebrae: vec![
                Vertebra {
                    rel_path: "content/a.typ".into(),
                    output_path: "doc.pdf".into(),
                    handle: "a".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "A".into(),
                    date: None,
                    vars: HashMap::new(),
                    source: String::new(),
                    sites: LabelSites::default(),
                },
                Vertebra {
                    rel_path: "content/b.typ".into(),
                    output_path: "doc.pdf".into(),
                    handle: "b".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "B".into(),
                    date: None,
                    vars: HashMap::new(),
                    source: String::new(),
                    sites: LabelSites::default(),
                },
            ],
            layout: SpineLayout::SingleCombined {
                output_name: "doc.pdf".into(),
                format: "pdf".into(),
            },
        };
        let src = spine.source();
        assert!(src.contains("#document(\"doc.pdf\", format: \"pdf\""));
        assert!(src.contains("#include \"content/a.typ\""));
        assert!(src.contains("#include \"content/b.typ\""));
        // Synthesized handle anchors are now injected into the combined document so
        // cross-vertebra `@handle` references resolve.
        assert!(src.contains("rheo-handle"));
        assert!(src.contains("<a>"));
        assert!(src.contains("<b>"));
    }

    #[test]
    fn single_combined_collision_check_passes() {
        let spine = VirtualSpine {
            vertebrae: vec![
                Vertebra {
                    rel_path: "a.typ".into(),
                    output_path: "book.pdf".into(),
                    handle: "a".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "A".into(),
                    date: None,
                    vars: Default::default(),
                    source: String::new(),
                    sites: LabelSites::default(),
                },
                Vertebra {
                    rel_path: "b.typ".into(),
                    output_path: "book.pdf".into(),
                    handle: "b".into(),
                    extra_handles: vec![],
                    emit_handle: true,
                    title: "B".into(),
                    date: None,
                    vars: Default::default(),
                    source: String::new(),
                    sites: LabelSites::default(),
                },
            ],
            layout: SpineLayout::SingleCombined {
                output_name: "book.pdf".into(),
                format: "pdf".into(),
            },
        };
        assert!(spine.check_output_collisions().is_ok());
    }

    #[test]
    fn canonical_skipped_when_user_label_conflicts() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // Source file hand-authors the same label as rheo would synthesize.
        fs::write(content.join("intro.typ"), "= Intro <intro>\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        // Without prefixing, the raw `<intro>` collides with the canonical handle.
        let spine = VirtualSpine::build(&files, &content, root, layout, false).unwrap();

        assert_eq!(spine.vertebrae[0].handle, "intro");
        // Canonical was user-claimed → not emitted.
        assert!(!spine.vertebrae[0].emit_handle);
        // Escape label still present.
        assert!(
            spine.vertebrae[0]
                .extra_handles
                .contains(&"intro.typ".to_string())
        );
    }

    #[test]
    fn escape_label_collision_returns_error() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // intro.typ hand-authors <intro.typ>, which is the escape alias for intro.typ itself.
        fs::write(content.join("intro.typ"), "= Intro <intro.typ>\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let result = VirtualSpine::build(&files, &content, root, layout, false);
        match result {
            Err(e) => {
                let msg = e.to_string();
                assert!(msg.contains("intro.typ"), "error should name label: {msg}");
            }
            Ok(_) => panic!("expected escape collision error"),
        }
    }

    #[test]
    fn prefixing_keeps_handle_anchor_when_user_label_matches() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // Hand-authored `<intro>` becomes `<intro:intro>` under prefixing, so it no
        // longer occupies the bare canonical handle.
        fs::write(content.join("intro.typ"), "= Intro <intro>\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let spine = VirtualSpine::build(&files, &content, root, layout, true).unwrap();

        // Canonical is not suppressed: the prefixed label can't equal the handle.
        assert!(spine.vertebrae[0].emit_handle);
    }

    #[test]
    fn prefixing_avoids_escape_collision_from_user_label() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        fs::create_dir_all(&content).unwrap();
        // `<intro.typ>` would collide with the escape alias in raw mode, but under
        // prefixing it becomes `<intro:intro.typ>` and no longer collides.
        fs::write(content.join("intro.typ"), "= Intro <intro.typ>\n").unwrap();

        let files = vec![content.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        assert!(VirtualSpine::build(&files, &content, root, layout, true).is_ok());
    }
}

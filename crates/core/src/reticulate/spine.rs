use crate::path_utils::{escape_typst_content, sanitize_handle_segment, to_forward_slash};
use crate::pdf_utils::DocumentTitle;
use crate::plugins::SpineOptions;
use crate::reticulate::parser;
use crate::reticulate::types::RheoValue;
use crate::{Result, RheoError, TYP_EXT};
use std::collections::HashMap;
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
            return Err(RheoError::project_config(
                "merge spine matched no .typ files",
            ));
        }

        Ok(typst_files)
    }
}

// ── Bundle spine: VirtualSpine, Vertebra, SpineLayout ────────────────────────

/// How a spine is compiled into output files under the bundle path.
pub enum SpineLayout {
    /// One output file per vertebra (e.g. HTML: "intro.html", "closing.html").
    OnePerVertebra { ext: String },
    /// All vertebrae in one combined output (e.g. PDF: "doc.pdf").
    SingleCombined { output_name: String },
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
    /// Document title for `#document title:` and `@handle` display text.
    pub title: String,
    /// Harvested `rheo-*` variables from this vertebra's source file.
    pub vars: std::collections::HashMap<String, RheoValue>,
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
    ) -> Result<Self> {
        // Stem relative to content_dir (no extension, forward-slash).
        let rel_stems: Vec<String> = files
            .iter()
            .map(|f| to_forward_slash(&f.strip_prefix(content_dir).unwrap_or(f).with_extension("")))
            .collect();

        // Count bare basenames to detect collisions.
        let mut basename_count: HashMap<String, usize> = HashMap::new();
        for rs in &rel_stems {
            let basename = rs.split('/').next_back().unwrap_or(rs).to_string();
            *basename_count.entry(basename).or_insert(0) += 1;
        }

        let vertebrae = files
            .iter()
            .zip(rel_stems.iter())
            .map(|(file, rel_stem)| {
                let basename = rel_stem
                    .split('/')
                    .next_back()
                    .unwrap_or(rel_stem)
                    .to_string();
                let sanitized_base = sanitize_handle_segment(&basename);
                let is_collision = basename_count.get(&basename).copied().unwrap_or(0) > 1;

                let handle = if is_collision {
                    // Path-qualified: "chapters/intro" → "chapters-intro".
                    rel_stem
                        .split('/')
                        .map(sanitize_handle_segment)
                        .collect::<Vec<_>>()
                        .join("-")
                } else {
                    sanitized_base.clone()
                };

                // <stem.typ> escape form is always included as a secondary alias.
                let extra_handles = vec![format!("{sanitized_base}.typ")];

                let output_path = match &layout {
                    SpineLayout::OnePerVertebra { ext } => {
                        // Flat output — directory separators become '_'.
                        let sanitized = sanitize_handle_segment(&rel_stem.replace('/', "_"));
                        format!("{sanitized}.{ext}")
                    }
                    SpineLayout::SingleCombined { output_name } => output_name.clone(),
                };

                let source = fs::read_to_string(file).unwrap_or_default();
                let stem = file
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let title = DocumentTitle::from_source(&source, &stem).extract();
                let rel_path = to_forward_slash(file.strip_prefix(project_root).unwrap_or(file));

                // Harvest rheo-* variables from the source file.
                let source_obj = Source::detached(&source);
                let extracted = parser::extract_nodes(&source_obj);
                let mut vars = HashMap::new();
                for v in extracted.rheo_vars {
                    match v.value {
                        Some(value) => {
                            vars.insert(v.key, value);
                        }
                        None => {
                            return Err(RheoError::invalid_data(format!(
                                "{}:{}: rheo-{} must be a string",
                                file.display(),
                                v.line,
                                v.key
                            )));
                        }
                    }
                }

                Ok(Vertebra {
                    rel_path,
                    output_path,
                    handle,
                    extra_handles,
                    title,
                    vars,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self { vertebrae, layout })
    }

    /// Validate that no two vertebrae produce the same output path.
    pub fn check_output_collisions(&self) -> Result<()> {
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
    /// For `SingleCombined` all includes are wrapped in one `#document`.
    ///
    /// The handle anchor uses `#figure` rather than `#metadata` or a bare label.
    /// `#metadata(none) <label>` fails at compile time ("cannot reference metadata");
    /// `<label>` after a `#document` block labels the document element itself, which
    /// Typst also refuses to reference ("cannot reference document"). A labeled
    /// `#figure([title], kind: "rheo-handle", …)` is the only mechanism that allows
    /// cross-document `@handle` resolution. The corresponding `#show figure.where(kind:
    /// "rheo-handle"): none` in rheo.typ suppresses its rendering.
    pub fn source(&self) -> String {
        let mut out = String::new();

        match &self.layout {
            SpineLayout::OnePerVertebra { .. } => {
                for v in &self.vertebrae {
                    let escaped = escape_typst_content(&v.title);
                    out.push_str(&format!(
                        "#document(\"{}\", title: [{escaped}])[\n",
                        v.output_path,
                    ));
                    // All handle aliases carry the title so @ref renders it as link text.
                    for label in std::iter::once(&v.handle).chain(v.extra_handles.iter()) {
                        out.push_str(&format!(
                            "  #figure([{escaped}], kind: \"rheo-handle\", supplement: none) <{label}>\n",
                        ));
                    }
                    out.push_str(&format!("  #include \"{}\"\n]\n\n", v.rel_path));
                }
            }
            SpineLayout::SingleCombined { output_name } => {
                let title = self
                    .vertebrae
                    .first()
                    .map(|v| v.title.as_str())
                    .unwrap_or("Document");
                out.push_str(&format!(
                    "#document(\"{output_name}\", title: [{}])[\n",
                    escape_typst_content(title),
                ));
                for v in &self.vertebrae {
                    out.push_str(&format!("  #include \"{}\"\n", v.rel_path));
                }
                out.push_str("]\n");
            }
        }

        out
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
            merge: false,
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
            merge: false,
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
                .contains("merge spine matched no .typ files")
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
        let layout = SpineLayout::OnePerVertebra { ext: "html".into() };
        let spine = VirtualSpine::build(&files, &content, root, layout).unwrap();

        assert_eq!(spine.vertebrae[0].handle, "intro");
        assert_eq!(spine.vertebrae[0].extra_handles, vec!["intro.typ"]);
        assert_eq!(spine.vertebrae[0].output_path, "intro.html");
        assert_eq!(spine.vertebrae[1].handle, "closing");
        assert_eq!(spine.vertebrae[1].output_path, "closing.html");
    }

    #[test]
    fn stem_collision_produces_path_qualified_handle() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        let chaps = content.join("chapters");
        let app = content.join("appendix");
        fs::create_dir_all(&chaps).unwrap();
        fs::create_dir_all(&app).unwrap();
        fs::write(chaps.join("intro.typ"), "").unwrap();
        fs::write(app.join("intro.typ"), "").unwrap();

        let files = vec![chaps.join("intro.typ"), app.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra { ext: "html".into() };
        let spine = VirtualSpine::build(&files, &content, root, layout).unwrap();

        assert_eq!(spine.vertebrae[0].handle, "chapters:intro");
        assert_eq!(spine.vertebrae[1].handle, "appendix:intro");
        assert!(
            spine.vertebrae[0]
                .extra_handles
                .contains(&"intro.typ".to_string())
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
            title: "Introduction".into(),
            vars: HashMap::new(),
        };
        let spine = VirtualSpine {
            vertebrae: vec![v],
            layout: SpineLayout::OnePerVertebra { ext: "html".into() },
        };
        let src = spine.source();
        assert!(src.contains("#document(\"intro.html\""));
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
                    title: "A".into(),
                    vars: HashMap::new(),
                },
                Vertebra {
                    rel_path: "content/b.typ".into(),
                    output_path: "doc.pdf".into(),
                    handle: "b".into(),
                    extra_handles: vec![],
                    title: "B".into(),
                    vars: HashMap::new(),
                },
            ],
            layout: SpineLayout::SingleCombined {
                output_name: "doc.pdf".into(),
            },
        };
        let src = spine.source();
        assert!(src.contains("#document(\"doc.pdf\""));
        assert!(src.contains("#include \"content/a.typ\""));
        assert!(src.contains("#include \"content/b.typ\""));
        assert!(!src.contains("rheo-handle"));
    }
}

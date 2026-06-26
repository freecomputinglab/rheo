use crate::path_utils::{escape_typst_content, sanitize_handle_segment, to_forward_slash};
use crate::pdf_utils::{DocumentTitle, sanitize_label_name};
use crate::plugins::{LinkStrategy, SpineOptions};
use crate::reticulate::transformer::LinkTransformer;
use crate::reticulate::types::RheoValue;
use crate::{Result, RheoError, TYP_EXT};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A spine with relative linking transformations.
#[derive(Debug, Clone)]
pub struct BuiltSpine {
    /// The name of the file or website that the spine will generate.
    pub title: Option<String>,

    /// Whether or not the source has been merged into a single file.
    /// This is only true for PDF merged mode.
    pub is_merged: bool,

    /// Reticulated (relative link transformed) source files.
    /// Always length 1 if `is_merged`.
    pub source: Vec<String>,

    /// Validated `rheo-*` vars (prefix stripped) per vertebra, aligned with the
    /// original `spine_files` order — one map per original file even when
    /// `is_merged` collapses `source` to length 1.
    pub vars: Vec<HashMap<String, RheoValue>>,
}

impl BuiltSpine {
    /// Build a RheoSpine with AST-based link transformation for all output formats.
    ///
    /// # Arguments
    /// * `root` - Project root directory
    /// * `spine_config` - Optional spine configuration (determines spine files)
    /// * `format_ext` - The extension to use for relative links.
    /// * `strategy` - How relative `.typ` links are rewritten (extension vs PDF labels)
    /// * `merge` - Whether to merge spine files into a single source (caller decides)
    pub fn build(
        root: &Path,
        spine_config: Option<&SpineOptions>,
        format_ext: &str,
        strategy: LinkStrategy,
        merge: bool,
    ) -> Result<BuiltSpine> {
        let spine_files = match spine_config {
            Some(spine) => spine.generate(root)?,
            None => collect_one_typst_file(root)?,
        };
        check_duplicate_filenames(&spine_files)?;

        // Merge when caller requests it (typically only PDF merged mode).
        // Other formats (epub, html) handle concatenation differently.

        // Paged formats attach the spine so cross-file links become labels;
        // a single file has no cross-references, so it's skipped.
        let mut transformer = LinkTransformer::new(format_ext)
            .with_strategy(strategy)
            .with_import_rewriting(merge);
        if strategy == LinkStrategy::PagedLabels && spine_files.len() > 1 {
            transformer = transformer.with_spine(spine_files.to_vec());
        }

        let mut sources = Vec::new();
        let mut vars = Vec::new();

        for spine_file in &spine_files {
            let source = fs::read_to_string(spine_file).map_err(|e| {
                RheoError::project_config(format!(
                    "failed to read spine file '{}': {}",
                    spine_file.display(),
                    e
                ))
            })?;

            let output = transformer.transform_with_vars(&source, spine_file, root)?;
            let transformed_source = output.source;

            // A `None` value means the RHS was an unsupported kind. Only string
            // literals are supported for now, so report it as such.
            let mut file_vars = HashMap::new();
            for v in output.rheo_vars {
                match v.value {
                    Some(value) => {
                        file_vars.insert(v.key, value);
                    }
                    None => {
                        return Err(RheoError::invalid_data(format!(
                            "{}:{}: rheo-{} must be a string",
                            spine_file.display(),
                            v.line,
                            v.key
                        )));
                    }
                }
            }
            vars.push(file_vars);

            let final_source = if merge {
                let (label, doc_title) = extract_label_and_title(&source, spine_file)?;
                format!(
                    "#metadata(\"{}\") <{}>\n{}\n\n",
                    doc_title, label, transformed_source
                )
            } else {
                transformed_source
            };

            sources.push(final_source);
        }

        let final_sources = if merge {
            vec![sources.join("\n\n")]
        } else {
            sources
        };

        let title = spine_config.and_then(|s| s.title.clone());

        Ok(BuiltSpine {
            title,
            is_merged: merge,
            source: final_sources,
            vars,
        })
    }
}

fn extract_label_and_title(source: &str, spine_file: &Path) -> Result<(String, String)> {
    let filename = spine_file.file_name().ok_or_else(|| {
        RheoError::project_config(format!(
            "invalid filename in spine: '{}'",
            spine_file.display()
        ))
    })?;

    let filename_str = filename.to_string_lossy();
    let stem = filename_str.strip_suffix(TYP_EXT).unwrap_or(&filename_str);
    let label = sanitize_label_name(stem);
    let title = DocumentTitle::from_source(source, stem).extract();

    Ok((label, title))
}

fn check_duplicate_filenames(spine_files: &[PathBuf]) -> Result<()> {
    let mut seen: HashMap<String, &PathBuf> = HashMap::new();

    for spine_file in spine_files {
        if let Some(filename) = spine_file.file_name() {
            let key = filename.to_string_lossy().into_owned();
            match seen.entry(key) {
                Entry::Occupied(e) => {
                    return Err(RheoError::project_config(format!(
                        "duplicate filename in spine: '{}' appears at both '{}' and '{}'",
                        filename.to_string_lossy(),
                        e.get().display(),
                        spine_file.display()
                    )));
                }
                Entry::Vacant(e) => {
                    e.insert(spine_file);
                }
            }
        }
    }

    Ok(())
}

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

fn collect_one_typst_file(root: &Path) -> Result<Vec<PathBuf>> {
    let typst_files = collect_typst_files(root);

    match typst_files.len() {
        0 => Err(RheoError::project_config("need at least one .typ file")),
        1 => Ok(typst_files),
        _ => Err(RheoError::project_config(
            "multiple .typ files found, specify spine configuration",
        )),
    }
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
                    // Path-qualified: "chapters/intro" → "chapters:intro".
                    rel_stem
                        .split('/')
                        .map(sanitize_handle_segment)
                        .collect::<Vec<_>>()
                        .join(":")
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

                Ok(Vertebra {
                    rel_path,
                    output_path,
                    handle,
                    extra_handles,
                    title,
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

    fn write_spine_dir(files: &[(&str, &str)]) -> TempDir {
        let temp = TempDir::new().unwrap();
        for (name, contents) in files {
            fs::write(temp.path().join(name), contents).unwrap();
        }
        temp
    }

    #[test]
    fn test_build_collects_rheo_vars_per_file() {
        let temp = write_spine_dir(&[
            ("a.typ", "#let rheo-feed-title = \"Alpha\"\n= A"),
            ("b.typ", "= B, no vars"),
        ]);
        let spine = spine_with_vertebrae(vec!["*.typ".to_string()]);
        let built = BuiltSpine::build(
            temp.path(),
            Some(&spine),
            "html",
            LinkStrategy::ExtensionRewrite,
            false,
        )
        .unwrap();

        assert_eq!(built.vars.len(), 2);
        assert_eq!(
            built.vars[0].get("feed-title"),
            Some(&RheoValue::Str("Alpha".to_string()))
        );
        assert!(built.vars[1].is_empty());
    }

    #[test]
    fn test_build_errors_on_non_string_rheo_var() {
        let temp = write_spine_dir(&[("a.typ", "#let rheo-x = 1\n= A")]);
        let spine = spine_with_vertebrae(vec!["*.typ".to_string()]);
        let result = BuiltSpine::build(
            temp.path(),
            Some(&spine),
            "html",
            LinkStrategy::ExtensionRewrite,
            false,
        );

        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("a.typ"), "message missing path: {msg}");
        assert!(
            msg.contains("rheo-x must be a string"),
            "message missing reason: {msg}"
        );
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
                },
                Vertebra {
                    rel_path: "content/b.typ".into(),
                    output_path: "doc.pdf".into(),
                    handle: "b".into(),
                    extra_handles: vec![],
                    title: "B".into(),
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

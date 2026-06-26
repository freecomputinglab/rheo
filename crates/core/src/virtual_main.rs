use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::pdf_utils::DocumentTitle;
use crate::{Result, RheoError};

/// How a spine is compiled into output files.
pub enum SpineLayout {
    /// One output file per vertebra (HTML: "intro.html", "b.html", …).
    OnePerVertebra { ext: String },
    /// All vertebrae in one combined output (PDF: "doc.pdf").
    SingleCombined { output_name: String },
}

/// Resolved metadata for one vertebra.
pub struct Vertebra {
    /// Path relative to the project root, forward-slash separated (for `#include`).
    pub rel_path: String,
    /// Output path key in VirtualFs (e.g. "intro.html").
    pub output_path: String,
    /// Primary synthesized handle label (e.g. "intro" or "chapters:intro").
    pub handle: String,
    /// Additional handle aliases: always includes the `<stem.typ>` escape form.
    pub extra_handles: Vec<String>,
    /// Document title (for `#document title:` and `@ref` display text).
    pub title: String,
}

/// Compute per-vertebra metadata from a list of spine files.
///
/// `content_dir` is the project's content root; stems are computed relative to it
/// so that `content/chapters/intro.typ` yields handle `intro` (or `chapters:intro`
/// on collision).
pub fn build_vertebrae(
    files: &[PathBuf],
    content_dir: &Path,
    project_root: &Path,
    layout: &SpineLayout,
) -> Result<Vec<Vertebra>> {
    // Compute relative stems (no extension) from content_dir.
    let rel_stems: Vec<String> = files
        .iter()
        .map(|f| {
            let base = f.strip_prefix(content_dir).unwrap_or(f);
            to_forward_slash(&base.with_extension(""))
        })
        .collect();

    // Detect basename collisions.
    let mut basename_count: HashMap<String, usize> = HashMap::new();
    for rs in &rel_stems {
        let basename = last_component(rs);
        *basename_count.entry(basename).or_insert(0) += 1;
    }

    files
        .iter()
        .zip(rel_stems.iter())
        .map(|(file, rel_stem)| {
            let basename = last_component(rel_stem);
            let sanitized_base = sanitize_handle_segment(&basename);
            let is_collision = basename_count.get(&basename).copied().unwrap_or(0) > 1;

            let primary = if is_collision {
                sanitize_handle_path(rel_stem)
            } else {
                sanitized_base.clone()
            };

            // <stem.typ> escape form is always included.
            let extra_handles = vec![format!("{sanitized_base}.typ")];

            // Output path: flat — dir separators become '_'.
            let output_path = match layout {
                SpineLayout::OnePerVertebra { ext } => {
                    let flat_stem = rel_stem.replace('/', "_");
                    let sanitized = sanitize_handle_segment(&flat_stem);
                    format!("{sanitized}.{ext}")
                }
                SpineLayout::SingleCombined { output_name } => output_name.clone(),
            };

            // Title from source or filename.
            let source = std::fs::read_to_string(file).unwrap_or_default();
            let stem = file
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let title = DocumentTitle::from_source(&source, &stem).extract();

            // rel_path for #include: relative to project root, forward-slash.
            let from_root = file.strip_prefix(project_root).unwrap_or(file);
            let rel_path = to_forward_slash(from_root);

            Ok(Vertebra {
                rel_path,
                output_path,
                handle: primary,
                extra_handles,
                title,
            })
        })
        .collect()
}

/// Synthesize the virtual main Typst source for a spine.
pub fn build_virtual_main(vertebrae: &[Vertebra], layout: &SpineLayout) -> String {
    let mut out = String::new();

    match layout {
        SpineLayout::OnePerVertebra { .. } => {
            for v in vertebrae {
                out.push_str(&format!(
                    "#document(\"{}\", title: [{}])[\n",
                    v.output_path,
                    escape_typst_content(&v.title),
                ));
                // Figure body = vertebra title so @ref renders it as link text.
                let escaped_title = escape_typst_content(&v.title);
                out.push_str(&format!(
                    "  #figure([{escaped_title}], kind: \"rheo-handle\", supplement: none) <{}>\n",
                    v.handle,
                ));
                for extra in &v.extra_handles {
                    out.push_str(&format!(
                        "  #figure([{escaped_title}], kind: \"rheo-handle\", supplement: none) <{}>\n",
                        extra,
                    ));
                }
                out.push_str(&format!("  #include \"{}\"\n]\n\n", v.rel_path));
            }
        }
        SpineLayout::SingleCombined { output_name } => {
            // PDF: one document, all includes, no per-vertebra handle anchors needed.
            let title = vertebrae
                .first()
                .map(|v| v.title.as_str())
                .unwrap_or("Document");
            out.push_str(&format!(
                "#document(\"{}\", title: [{}])[\n",
                output_name,
                escape_typst_content(title),
            ));
            for v in vertebrae {
                out.push_str(&format!("  #include \"{}\"\n", v.rel_path));
            }
            out.push_str("]\n");
        }
    }

    out
}

/// Validate that no two vertebrae would produce the same output path.
pub fn check_output_path_collisions(vertebrae: &[Vertebra]) -> Result<()> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (i, v) in vertebrae.iter().enumerate() {
        if let Some(prev) = seen.insert(v.output_path.as_str(), i) {
            return Err(RheoError::project_config(format!(
                "spine output path collision: '{}' produced by both vertebra {} and {}",
                v.output_path, prev, i
            )));
        }
    }
    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Sanitize one label segment: keep alphanumeric, `-`, `_`; replace rest with `_`.
fn sanitize_handle_segment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Convert a relative path (dir/stem with no extension) to a path-qualified
/// handle by joining components with `:`.
fn sanitize_handle_path(rel_stem: &str) -> String {
    rel_stem
        .split('/')
        .map(sanitize_handle_segment)
        .collect::<Vec<_>>()
        .join(":")
}

fn last_component(rel_stem: &str) -> String {
    rel_stem
        .split('/')
        .next_back()
        .unwrap_or(rel_stem)
        .to_string()
}

fn to_forward_slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// Escape text for use inside Typst square-bracket content `[…]`.
/// Escapes `\`, `[`, `]`, `#`.
fn escape_typst_content(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('#', "\\#")
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_stems_get_bare_handle() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(content.join("intro.typ"), "= Intro\n").unwrap();
        std::fs::write(content.join("closing.typ"), "= Closing\n").unwrap();

        let files = vec![content.join("intro.typ"), content.join("closing.typ")];
        let layout = SpineLayout::OnePerVertebra { ext: "html".into() };
        let verts = build_vertebrae(&files, &content, root, &layout).unwrap();

        assert_eq!(verts[0].handle, "intro");
        assert_eq!(verts[0].extra_handles, vec!["intro.typ"]);
        assert_eq!(verts[0].output_path, "intro.html");

        assert_eq!(verts[1].handle, "closing");
        assert_eq!(verts[1].output_path, "closing.html");
    }

    #[test]
    fn stem_collision_produces_path_qualified_handle() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let content = root.join("content");
        let chaps = content.join("chapters");
        let app = content.join("appendix");
        std::fs::create_dir_all(&chaps).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(chaps.join("intro.typ"), "").unwrap();
        std::fs::write(app.join("intro.typ"), "").unwrap();

        let files = vec![chaps.join("intro.typ"), app.join("intro.typ")];
        let layout = SpineLayout::OnePerVertebra { ext: "html".into() };
        let verts = build_vertebrae(&files, &content, root, &layout).unwrap();

        assert_eq!(verts[0].handle, "chapters:intro");
        assert_eq!(verts[1].handle, "appendix:intro");
        // stem.typ escape still uses the bare basename
        assert!(verts[0].extra_handles.contains(&"intro.typ".to_string()));
        // output paths are flat and collision-free
        assert_ne!(verts[0].output_path, verts[1].output_path);
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
        let layout = SpineLayout::OnePerVertebra { ext: "html".into() };
        let src = build_virtual_main(&[v], &layout);

        assert!(src.contains("#document(\"intro.html\""));
        assert!(src.contains("rheo-handle"));
        assert!(src.contains("[Introduction]"));
        assert!(src.contains("<intro>"));
        assert!(src.contains("<intro.typ>"));
        assert!(src.contains("#include \"content/intro.typ\""));
    }

    #[test]
    fn virtual_main_pdf_shape() {
        let verts = vec![
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
        ];
        let layout = SpineLayout::SingleCombined {
            output_name: "doc.pdf".into(),
        };
        let src = build_virtual_main(&verts, &layout);

        assert!(src.contains("#document(\"doc.pdf\""));
        assert!(src.contains("#include \"content/a.typ\""));
        assert!(src.contains("#include \"content/b.typ\""));
        // no per-vertebra handle anchors in combined mode
        assert!(!src.contains("rheo-handle"));
    }
}

mod server;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

/// Output filename for the bundled default stylesheet when no user CSS resolves.
/// Distinct from `style.css` so it never clashes with a user's own stylesheet.
pub const DEFAULT_STYLESHEET_NAME: &str = "rheo-default.css";

use rayon::prelude::*;
use rheo_core::{
    AssetConfig, CastVertebra, EmbeddedDefault, FormatInitTemplate, FormatPlugin, LiveReload,
    OpenHandle, PluginContext, ReloadKind, Result, RheoError, ServedPage, ServerHandle,
};
use std::collections::HashSet;
use std::path::Path;
use tracing::{debug, info, warn};

/// Reload callback type - called by watch loop after successful compilation.
pub type ReloadCallback = Box<dyn Fn(ReloadKind) + Send + Sync>;

/// Server handle for HTML plugin's development server
pub struct HtmlServerHandle {
    pub runtime: tokio::runtime::Runtime,
    pub server_task: tokio::task::JoinHandle<()>,
    pub url: String,
    pub reload_callback: ReloadCallback,
    pub vfs_arc: std::sync::Arc<tokio::sync::RwLock<Option<typst_bundle::VirtualFs>>>,
}

impl ServerHandle for HtmlServerHandle {
    fn url(&self) -> &str {
        &self.url
    }
    fn reload(&self, kind: ReloadKind) {
        (self.reload_callback)(kind);
    }
    fn update_virtual_fs(&self, vfs: typst_bundle::VirtualFs) {
        let arc = self.vfs_arc.clone();
        self.runtime.spawn(async move {
            *arc.write().await = Some(vfs);
        });
    }
    fn forget_virtual_paths(&self, paths: &[String]) {
        let arc = self.vfs_arc.clone();
        let paths = paths.to_vec();
        self.runtime.spawn(async move {
            if let Some(vfs) = arc.write().await.as_mut() {
                forget_paths(vfs, &paths);
            }
        });
    }
}

/// Drop every entry whose relative path is in `paths`, so the dev server
/// falls through to disk for them instead of serving a stale in-memory copy.
fn forget_paths(vfs: &mut typst_bundle::VirtualFs, paths: &[String]) {
    let drop: HashSet<&str> = paths.iter().map(String::as_str).collect();
    vfs.retain(|vpath, _| !drop.contains(vpath.get_with_slash().trim_start_matches('/')));
}

pub struct HtmlPlugin;

const PLUGIN_NAME: &str = "html";
const STYLESHEETS: &str = "css_stylesheet";
const SCRIPTS: &str = "js_scripts";

impl FormatPlugin for HtmlPlugin {
    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    fn format_init_template(&self) -> FormatInitTemplate {
        FormatInitTemplate {
            files: vec![
                ("style.css", include_str!("templates/style.css")),
                ("index.js", include_str!("templates/index.js")),
            ],
            options_toml: None,
        }
    }

    fn live_reload(&self) -> Option<&dyn LiveReload> {
        Some(self)
    }

    fn open(&self, output_dir: &Path, _format_name: &str) -> Result<OpenHandle> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| RheoError::io(e, "creating tokio runtime"))?;

        let (server_task, reload_tx, url, vfs_arc) = runtime
            .block_on(async { server::start_server(output_dir.to_path_buf(), 3000).await })?;

        if let Err(e) = server::open_browser(&url) {
            warn!(error = %e, "failed to open browser, but server is running");
        }

        let reload_callback: ReloadCallback = Box::new(move |kind| {
            let _ = reload_tx.send(kind);
        });

        let handle = HtmlServerHandle {
            runtime,
            server_task,
            url,
            reload_callback,
            vfs_arc,
        };
        Ok(OpenHandle::Server(Box::new(handle)))
    }

    fn assets(&self) -> Vec<AssetConfig> {
        vec![
            AssetConfig {
                name: STYLESHEETS,
                default_path: "style.css",
                required: false,
                // No user/override stylesheet → ship the built-in default as a
                // real linked file (rheo-default.css) rather than an inline <style>.
                default_content: Some(EmbeddedDefault {
                    name: DEFAULT_STYLESHEET_NAME,
                    content: DEFAULT_STYLESHEET,
                }),
            },
            AssetConfig {
                name: SCRIPTS,
                default_path: "index.js",
                required: false,
                default_content: None,
            },
        ]
    }

    fn compile(&self, ctx: PluginContext<'_>, outputs: &[CastVertebra]) -> Result<()> {
        // EVERY DIRECTORY FIRST, distinct, then the files. Per page this was one
        // `create_dir_all` syscall for a directory that already existed — 360 of
        // them on a project with 11 output directories — and hoisting it is also
        // what lets the writes below run in any order.
        let mut dirs: HashSet<&Path> = HashSet::new();
        for output in outputs {
            if let Some(parent) = Path::new(&output.output_path).parent()
                && !parent.as_os_str().is_empty()
            {
                dirs.insert(parent);
            }
        }
        for dir in dirs {
            let dest = ctx.output_dir.join(dir);
            std::fs::create_dir_all(&dest)
                .map_err(|e| RheoError::io(e, format!("creating output directory {:?}", dest)))?;
        }
        // The output root itself, for the pages that sit directly in it.
        std::fs::create_dir_all(ctx.output_dir).map_err(|e| {
            RheoError::io(e, format!("creating output directory {:?}", ctx.output_dir))
        })?;

        // IN PARALLEL, one page per task. The measurable cost of this phase is
        // NOT the write syscall — it is `html_string`, the `<head>` injection
        // and `rewrite_page` running once per page, all of them pure functions
        // of one page — so the pages are independent and the phase scales with
        // cores. The directories above are the only ordering requirement, which
        // is why they are a separate pass.
        //
        // Every project image path a page's placeholders name, gathered
        // across the parallel loop below so it can be copied once after —
        // several pages commonly reference the same image.
        let image_paths: std::sync::Mutex<HashSet<String>> = std::sync::Mutex::new(HashSet::new());

        // `try_fold`/`try_reduce` rather than `for_each`: a write error must
        // still be returned, and the first one is enough.
        let (written, unchanged) = outputs
            .par_iter()
            .map(|output| -> Result<(usize, usize)> {
                let html_string = output.html_string()?;
                // The same finishing the dev server serves, so `rheo watch` and
                // `rheo compile` never disagree about a page's `<head>`.
                let page = ctx.page.page(&output.output_path, &html_string);
                if let Some((_, paths)) =
                    rheo_core::image_assets::resolve_placeholders(&html_string, &output.output_path)
                {
                    image_paths
                        .lock()
                        .expect("image_paths mutex poisoned")
                        .extend(paths);
                }
                let html_string = self.rewrite_page(&page)?.unwrap_or(html_string);

                let out_path = ctx.output_dir.join(&output.output_path);

                // COMPARE AGAINST THE FILE ALREADY THERE, and skip an identical
                // write. Typst's memo means most pages of a rebuild come out
                // byte-identical, and rewriting them buys nothing while costing
                // their mtimes — so every downstream watcher (a browser
                // livereload, an rsync deploy, a second rheo watching the
                // output) treats the whole site as changed. MEASURED, the skip
                // does not make the phase faster: reading a page back costs
                // about what writing it does. It is here for the mtimes and for
                // the `written`/`unchanged` counts below, which say whether an
                // edit reached the pages it was meant to.
                //
                // No manifest and no content-hash sidecar: the file already on
                // disk IS the record, and a second copy of it is a second thing
                // to keep true. A read failure — no file yet, or unreadable —
                // simply means "write it".
                if std::fs::read(&out_path)
                    .map(|old| old == html_string.as_bytes())
                    .unwrap_or(false)
                {
                    debug!(output = %out_path.display(), "unchanged, not rewritten");
                    return Ok((0, 1));
                }

                std::fs::write(&out_path, &html_string).map_err(|e| {
                    RheoError::io(e, format!("writing HTML file to {:?}", out_path))
                })?;
                // DEBUG, not info: this fires once per page, and a project
                // emitting 360 of them buried every line that mattered under
                // 360 that did not. The counts below are what INFO carries.
                debug!(
                    output = %out_path.display(),
                    size = html_string.len(),
                    "wrote HTML page"
                );
                Ok((1, 0))
            })
            .try_reduce(|| (0, 0), |a, b| Ok((a.0 + b.0, a.1 + b.1)))?;

        // Publish each referenced project image once, at its project-root-
        // relative path — the same place a `[[html.assets]] copy` glob would
        // put it, and unaffected by `content_dir`. Skipped when the copy
        // already there is current, so a watch rebuild does not re-copy
        // every image on every pass.
        for path in image_paths
            .into_inner()
            .expect("image_paths mutex poisoned")
        {
            let src = ctx.bundle.project.root.join(&path);
            let dest = ctx.output_dir.join(&path);
            if rheo_core::assets::dest_is_current(&src, &dest) {
                continue;
            }
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    RheoError::io(e, format!("creating image output directory {:?}", parent))
                })?;
            }
            std::fs::copy(&src, &dest)
                .map_err(|e| RheoError::io(e, format!("copying image {:?} to {:?}", src, dest)))?;
        }

        // ONE LINE, and on a watch rebuild it is the most useful one there is:
        // "written=3 unchanged=357" says immediately whether an edit reached the
        // pages it was supposed to reach.
        info!(written, unchanged, "wrote HTML output");
        Ok(())
    }
}

impl LiveReload for HtmlPlugin {
    /// Link this build's stylesheets and scripts (depth-relative to the page)
    /// plus any site-wide head fragment into the page's `<head>`. A non-HTML
    /// output is left alone.
    fn rewrite_page(&self, page: &ServedPage<'_>) -> Result<Option<String>> {
        if !page.path.ends_with(".html") {
            return Ok(None);
        }

        let resolved = rheo_core::image_assets::resolve_placeholders(page.text, page.path);
        let text = resolved
            .as_ref()
            .map_or(page.text, |(text, _)| text.as_str());

        let built_paths = |name| {
            page.assets
                .get(&name)
                .map(|assets: &Vec<rheo_core::Asset>| assets.as_slice())
                .unwrap_or_default()
        };
        let css_paths: Vec<String> = built_paths(STYLESHEETS)
            .iter()
            .map(|a| a.built_relative_path.clone())
            .collect();
        let js_scripts: Vec<rheo_core::html_dom::ScriptRef> = built_paths(SCRIPTS)
            .iter()
            .map(|a| rheo_core::html_dom::ScriptRef {
                src: a.built_relative_path.clone(),
                module: a.module,
                rehydrate: a.rehydrate,
            })
            .collect();

        let css = rheo_core::html_dom::depth_relative_refs(&css_paths, page.path);
        let js = rheo_core::html_dom::depth_relative_scripts(&js_scripts, page.path);
        let mutated = rheo_core::html_dom::HtmlDom::apply_head_mutations(
            text,
            &css,
            &js,
            page.head_fragment,
        )?;
        // A page whose only change is its images still needs rewriting, even
        // when the head mutations found nothing to do.
        Ok(mutated.or_else(|| resolved.map(|(text, _)| text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forget_paths_drops_only_named_entries() {
        use typst::foundations::Bytes;
        use typst::syntax::VirtualPath;

        let mut vfs = typst_bundle::VirtualFs::default();
        vfs.insert(
            VirtualPath::new("a.html").expect("valid virtual path"),
            Bytes::new(b"a".to_vec()),
        );
        vfs.insert(
            VirtualPath::new("b.html").expect("valid virtual path"),
            Bytes::new(b"b".to_vec()),
        );
        vfs.insert(
            VirtualPath::new("style.css").expect("valid virtual path"),
            Bytes::new(b"css".to_vec()),
        );

        forget_paths(&mut vfs, &["b.html".to_string()]);

        let remaining: HashSet<String> = vfs
            .keys()
            .map(|p| p.get_with_slash().trim_start_matches('/').to_string())
            .collect();
        assert_eq!(
            remaining,
            HashSet::from(["a.html".to_string(), "style.css".to_string()])
        );
    }

    /// The site-wide head fragment (populated by `ControlAssets::extract` from
    /// a bundle-root `.rheo/head.html` control asset, before the plugin ever
    /// sees it) must land in the `<head>` of *every* compiled page, not just
    /// one — that's the whole point of a site-wide control asset, as opposed
    /// to a per-page `<rheo-head>` wrapper.
    #[test]
    fn test_compile_appends_control_head_fragment_to_every_page() {
        use rheo_core::project::{ProjectConfig, ProjectMode};
        use rheo_core::reticulate::{SpineLayout, SpineScan, VirtualSpine};
        use rheo_core::{ControlAssets, PluginSection, RheoConfig, TypstFormat};
        use std::collections::HashMap;
        use typst::foundations::Bytes;

        let dir = tempfile::tempdir().expect("tempdir");
        let output_dir = dir.path().to_path_buf();

        let make = |path: &str| CastVertebra {
            output_path: path.to_string(),
            bytes: Bytes::new(
                format!("<html><head><title>{path}</title></head><body><p>hi</p></body></html>")
                    .into_bytes(),
            ),
            format: TypstFormat::Html,
            title: path.to_string(),
            date: None,
            description: None,
            keywords: vec![],
            author: vec![],
            contributed: false,
        };
        let outputs = vec![make("a.html"), make("sub/b.html")];

        let project = ProjectConfig {
            name: "test".to_string(),
            root: output_dir.clone(),
            config: RheoConfig::default(),
            typ_files: vec![],
            mode: ProjectMode::Directory,
            config_path: None,
        };
        let section = PluginSection::default();
        let assets = HashMap::new();
        let layout = SpineLayout::OnePerVertebra {
            ext: "html".into(),
            format: "html".into(),
        };
        let virtual_spine =
            VirtualSpine::build(SpineScan::flat(&[], &output_dir), &output_dir, layout)
                .expect("build empty spine");
        let bundle_assets: Vec<(String, Bytes)> = vec![];
        let control = ControlAssets {
            head_fragment: Some(r#"<meta name="site-wide" content="yes">"#.to_string()),
        };
        let ctx = PluginContext {
            output_dir: &output_dir,
            config: &section,
            page: rheo_core::PageAssets {
                assets: &assets,
                head_fragment: control.head_fragment.as_deref(),
            },
            bundle: rheo_core::BundleInputs {
                project: &project,
                spine: &virtual_spine,
                assets: &bundle_assets,
            },
        };

        HtmlPlugin.compile(ctx, &outputs).expect("compile");

        for path in ["a.html", "sub/b.html"] {
            let written = std::fs::read_to_string(output_dir.join(path)).expect("read output");
            assert!(
                written.contains("site-wide"),
                "page {path} missing site-wide head fragment:\n{written}"
            );
            let head_end = written.find("</head>").expect("output has a <head>");
            let frag_pos = written.find("site-wide").unwrap();
            assert!(
                frag_pos < head_end,
                "fragment must land inside <head> for {path}"
            );
        }
    }

    /// A build resolves its stylesheets once and every page links all of them,
    /// so `rewrite_page` must emit one `<link>` per resolved stylesheet rather
    /// than stopping at the first.
    #[test]
    fn test_rewrite_page_links_every_stylesheet() {
        use std::collections::HashMap;

        let stylesheet = |name: &str| rheo_core::Asset {
            config: AssetConfig {
                name: STYLESHEETS,
                default_path: "style.css",
                required: false,
                default_content: None,
            },
            module: false,
            rehydrate: false,
            source_path: Path::new("/project").join(name),
            resolved_path: Path::new("/build/html").join(name),
            built_relative_path: name.to_string(),
        };
        let assets: HashMap<&'static str, Vec<rheo_core::Asset>> = HashMap::from([(
            STYLESHEETS,
            vec![stylesheet("one.css"), stylesheet("two.css")],
        )]);

        let page = ServedPage {
            path: "index.html",
            text: "<html><head><title>t</title></head><body><p>hi</p></body></html>",
            assets: &assets,
            head_fragment: None,
        };

        let rewritten = HtmlPlugin
            .rewrite_page(&page)
            .expect("rewrite succeeds")
            .expect("an .html page is rewritten");
        for name in ["one.css", "two.css"] {
            let link = format!(r#"<link rel="stylesheet" href="{name}">"#);
            assert!(
                rewritten.contains(&link),
                "page missing {link}:\n{rewritten}"
            );
        }
    }

    /// A page whose only change is a placeholder image still gets rewritten,
    /// relative to its own depth.
    #[test]
    fn test_rewrite_page_resolves_image_placeholders() {
        use std::collections::HashMap;

        let assets: HashMap<&'static str, Vec<rheo_core::Asset>> = HashMap::new();

        let page = ServedPage {
            path: "sub/page.html",
            text: r#"<html><head><title>t</title></head><body><img src="rheo-asset:/img/a.png"></body></html>"#,
            assets: &assets,
            head_fragment: None,
        };

        let rewritten = HtmlPlugin
            .rewrite_page(&page)
            .expect("rewrite succeeds")
            .expect("an .html page with a placeholder is rewritten");
        assert!(
            rewritten.contains(r#"src="../img/a.png""#),
            "image src not resolved relative to page depth:\n{rewritten}"
        );
    }

    /// The dev-server path serves what the on-disk path writes: core compiles
    /// into memory, then hands each page to this plugin's [`LiveReload`], so a
    /// `<rheo-head>` wrapper is hoisted and the build's stylesheet is linked in
    /// the served bytes too.
    #[test]
    fn test_compile_for_watch_hoists_and_links_like_the_on_disk_path() {
        use rheo_core::project::{ProjectConfig, ProjectMode};
        use rheo_core::{Build, BuildOptions, RheoConfig};

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("content")).expect("create content dir");
        std::fs::write(
            root.join("content/a.typ"),
            "= Page A\n#html.elem(\"rheo-head\", html.elem(\"meta\", attrs: (name: \"x\", content: \"y\")))\n",
        )
        .expect("write content/a.typ");

        let project = ProjectConfig {
            name: "test".to_string(),
            root: root.to_path_buf(),
            config: RheoConfig {
                content_dir: Some("content".to_string()),
                formats: vec![PLUGIN_NAME.to_string()],
                ..Default::default()
            },
            typ_files: vec![root.join("content/a.typ")],
            mode: ProjectMode::Directory,
            config_path: None,
        };

        let plugin: Box<dyn FormatPlugin> = Box::new(HtmlPlugin);
        let build =
            Build::prepare(project, vec![plugin], BuildOptions::default()).expect("prepare build");
        let vfs = build
            .compile_for_watch()
            .expect("compile_for_watch")
            .expect("html declares the live-reload capability");

        let (_, bytes) = vfs
            .iter()
            .find(|(p, _)| p.get_with_slash().ends_with("a.html"))
            .expect("a.html present");
        let html = String::from_utf8_lossy(bytes.as_slice());
        assert!(!html.contains("rheo-head"), "wrapper not removed:\n{html}");
        assert!(
            html.contains(DEFAULT_STYLESHEET_NAME),
            "css link missing:\n{html}"
        );
        let head_end = html.find("</head>").expect("has head");
        let meta_pos = html.find("name=\"x\"").expect("meta present");
        assert!(meta_pos < head_end, "meta not hoisted into head:\n{html}");
    }

    /// A full on-disk build publishes a referenced project image as a file,
    /// once, at its project-root-relative path, and links it relative to the
    /// page with no `rheo-asset:` placeholder left behind. A second build
    /// leaves the copy's mtime alone, since `dest_is_current` sees nothing
    /// changed. Lives here (not `rheo-core`) because the rewrite and copy
    /// both happen inside `HtmlPlugin`, which `rheo-core` cannot depend on.
    #[test]
    fn image_assets_publishes_files_once() {
        use rheo_core::project::{ProjectConfig, ProjectMode};
        use rheo_core::{Build, BuildOptions, RheoConfig};

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("content/img")).expect("create content/img dir");
        std::fs::write(root.join("content/a.typ"), "#image(\"img/a.svg\")\n")
            .expect("write content/a.typ");
        std::fs::write(
            root.join("content/img/a.svg"),
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="2"></svg>"#,
        )
        .expect("write svg");

        let make_project = || ProjectConfig {
            name: "test".to_string(),
            root: root.to_path_buf(),
            config: RheoConfig {
                content_dir: Some("content".to_string()),
                formats: vec![PLUGIN_NAME.to_string()],
                ..Default::default()
            },
            typ_files: vec![root.join("content/a.typ")],
            mode: ProjectMode::Directory,
            config_path: None,
        };

        let plugin: Box<dyn FormatPlugin> = Box::new(HtmlPlugin);
        Build::prepare(make_project(), vec![plugin], BuildOptions::default())
            .expect("prepare build")
            .run()
            .expect("run build");

        let html_path = root.join("build/html/a.html");
        let html = std::fs::read_to_string(&html_path).expect("read a.html");
        assert!(
            html.contains(r#"src="content/img/a.svg""#),
            "image src not resolved:\n{html}"
        );
        assert!(
            !html.contains("rheo-asset:"),
            "placeholder left in:\n{html}"
        );

        let image_path = root.join("build/html/content/img/a.svg");
        let first_copy_mtime = image_path
            .metadata()
            .expect("copied image exists")
            .modified()
            .expect("mtime");

        // Run again: the copy is already current, so it is left untouched.
        let plugin: Box<dyn FormatPlugin> = Box::new(HtmlPlugin);
        Build::prepare(make_project(), vec![plugin], BuildOptions::default())
            .expect("prepare build")
            .run()
            .expect("run build");
        let second_copy_mtime = image_path
            .metadata()
            .expect("copied image still exists")
            .modified()
            .expect("mtime");
        assert_eq!(
            first_copy_mtime, second_copy_mtime,
            "image was re-copied on an unchanged rebuild"
        );
    }
}

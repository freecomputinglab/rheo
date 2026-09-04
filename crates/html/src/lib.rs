mod server;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

/// Output filename for the bundled default stylesheet when no user CSS resolves.
/// Distinct from `style.css` so it never clashes with a user's own stylesheet.
pub const DEFAULT_STYLESHEET_NAME: &str = "rheo-default.css";

use rheo_core::{
    AssetConfig, CastVertebra, EmbeddedDefault, FormatInitTemplate, FormatPlugin, LiveReload,
    OpenHandle, PluginContext, Result, RheoError, ServedPage, ServerHandle,
};
use std::path::Path;
use tracing::{debug, info, warn};

/// Reload callback type - called by watch loop after successful compilation.
pub type ReloadCallback = Box<dyn Fn() + Send + Sync>;

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
    fn reload(&self) {
        (self.reload_callback)();
    }
    fn update_virtual_fs(&self, vfs: typst_bundle::VirtualFs) {
        let arc = self.vfs_arc.clone();
        self.runtime.spawn(async move {
            *arc.write().await = Some(vfs);
        });
    }
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

        let reload_callback: ReloadCallback = Box::new(move || {
            let _ = reload_tx.send(());
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
        for output in outputs {
            let html_string = output.html_string()?;
            // The same finishing the dev server serves, so `rheo watch` and
            // `rheo compile` never disagree about a page's `<head>`.
            let page = ctx.page.page(&output.output_path, &html_string);
            let html_string = self.rewrite_page(&page)?.unwrap_or(html_string);

            let out_path = ctx.output_dir.join(&output.output_path);
            debug!(size = html_string.len(), "writing HTML file");
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    RheoError::io(e, format!("creating output directory {:?}", parent))
                })?;
            }
            std::fs::write(&out_path, &html_string)
                .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", out_path)))?;
            info!(output = %out_path.display(), "successfully compiled to HTML");
        }

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
            })
            .collect();

        let css = rheo_core::html_dom::depth_relative_refs(&css_paths, page.path);
        let js = rheo_core::html_dom::depth_relative_scripts(&js_scripts, page.path);
        rheo_core::html_dom::HtmlDom::apply_head_mutations(page.text, &css, &js, page.head_fragment)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

mod server;

/// Bundled default HTML stylesheet.
/// Used when the project doesn't provide its own style.css.
pub const DEFAULT_STYLESHEET: &str = include_str!("templates/style.css");

/// Output filename for the bundled default stylesheet when no user CSS resolves.
/// Distinct from `style.css` so it never clashes with a user's own stylesheet.
pub const DEFAULT_STYLESHEET_NAME: &str = "rheo-default.css";

use rheo_core::{
    AssetConfig, CastVertebra, EmbeddedDefault, FormatInitTemplate, FormatPlugin, OpenHandle,
    PluginContext, Result, RheoError, ServerHandle,
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
        let css_assets = ctx.assets.get(&STYLESHEETS).filter(|v| !v.is_empty());
        let js_assets = ctx.assets.get(&SCRIPTS).filter(|v| !v.is_empty());

        let css_paths: Vec<String> = css_assets
            .map(|assets| {
                assets
                    .iter()
                    .inspect(|a| info!("Found CSS stylesheet: {}", a.resolved_path.display()))
                    .map(|a| a.built_relative_path.clone())
                    .collect()
            })
            .unwrap_or_default();

        let js_paths: Vec<String> = js_assets
            .map(|v| v.iter().map(|a| a.built_relative_path.clone()).collect())
            .unwrap_or_default();

        // A bundle-root `.rheo/head.html` control asset (see
        // `ControlAssets`), if present, contributes to every page's `<head>`
        // rather than just one page's — read it once up front.
        let head_fragment = ctx.control.head_fragment.as_deref();

        for output in outputs {
            let html_string = output.html_string()?;
            // Assets are written at the output root; make each ref
            // depth-relative so nested pages resolve them.
            let prefix = rheo_core::util::html::depth_prefix(&output.output_path);
            let css_refs: Vec<String> = css_paths.iter().map(|s| format!("{prefix}{s}")).collect();
            let js_refs: Vec<String> = js_paths.iter().map(|s| format!("{prefix}{s}")).collect();
            let css: Vec<&str> = css_refs.iter().map(|s| s.as_str()).collect();
            let js: Vec<&str> = js_refs.iter().map(|s| s.as_str()).collect();
            let html_string = rheo_core::util::html::HtmlDom::apply_head_mutations(
                &html_string,
                &css,
                &js,
                head_fragment,
            )?
            .unwrap_or(html_string);

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

#[cfg(test)]
mod tests {
    use super::*;

    /// `ctx.control.head_fragment` (populated by `ControlAssets::extract` from
    /// a bundle-root `.rheo/head.html` control asset, before the plugin ever
    /// sees it) must land in the `<head>` of *every* compiled page, not just
    /// one — that's the whole point of a site-wide control asset, as opposed
    /// to a per-page `<rheo-head>` wrapper.
    #[test]
    fn test_compile_appends_control_head_fragment_to_every_page() {
        use rheo_core::config::project::{ProjectConfig, ProjectMode};
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
        let font_dirs: Vec<std::path::PathBuf> = vec![];
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
            project: &project,
            output_dir: &output_dir,
            spine: &virtual_spine,
            config: &section,
            assets: &assets,
            font_dirs: &font_dirs,
            bundle_assets: &bundle_assets,
            control: &control,
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
}

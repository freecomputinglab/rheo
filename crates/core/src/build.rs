//! The build orchestrator.
//!
//! [`Build`] owns the full compile pipeline — format selection, smart defaults,
//! asset resolution, spine handling, and per-format compilation — independent of
//! the CLI. The `rheo` binary is a thin wrapper that maps command-line arguments
//! to [`BuildOptions`] and calls [`Build::prepare`] then [`Build::run`].

use crate::assets::AssetResolver;
use crate::compile::export_bundle;
use crate::config::PluginSection;
use crate::config::output::OutputConfig;
use crate::config::project::{ProjectConfig, ProjectMode};
use crate::diagnostics::results::CompilationResults;
use crate::plugins::{
    CastVertebra, DocumentMeta, FormatPlugin, PluginContext, TypstFormat, spine_layout_for,
};
use crate::reticulate::spine::{SpineScan, VirtualSpine};
use crate::transclude::{ContentTransclusion, ControlAssetKind, ControlAssets};
use crate::world::RheoWorld;
use crate::{Result, RheoError};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};
use typst::model::Document as _;

/// Inputs for preparing a [`Build`], typically mapped from CLI flags and config.
///
/// Every field defaults to "unset" — no formats requested, no directory
/// overrides, no debug artifacts — so library callers should set only what they
/// mean and spread the rest (`..BuildOptions::default()`), which keeps later
/// field additions from breaking them.
#[derive(Default)]
pub struct BuildOptions {
    /// Format names explicitly requested (e.g. from `--html`). Empty means
    /// "fall back to the config `formats` list, then to all plugins".
    pub formats: Vec<String>,
    /// Build output directory override (CLI `--build-dir`); resolved against the
    /// current working directory. `None` falls back to config, then default.
    pub build_dir: Option<PathBuf>,
    /// Additional font directories from `--font-dir`, appended on top of the
    /// autoscan/config-derived directories.
    pub font_dirs: Vec<PathBuf>,
    /// `--emit-bundle-source`: write each plugin's synthesized bundle main to
    /// `<build_dir>/<plugin>/.rheo-bundle.typ`. A read-only debug artifact —
    /// never read back — for diagnosing marrow/spine authoring errors. Off by
    /// default.
    pub emit_bundle_source: bool,
}

/// A prepared, runnable build: a project, the selected plugins, the resolved
/// output location, and the resolved font directories.
///
/// Construct with [`Build::prepare`] (format selection + smart defaults) and
/// execute with [`Build::run`]. This is the library entry point — no CLI is
/// required to compile a project.
pub struct Build {
    project: ProjectConfig,
    plugins: Vec<Box<dyn FormatPlugin>>,
    output: OutputConfig,
    font_dirs: Vec<PathBuf>,
    emit_bundle_source: bool,
}

/// The result of [`Build::compile_spine`]: the built spine, the compiled
/// bundle's flattened files, which of those files are raw assets, the
/// optional debug bundle source, and each document's Typst-resolved metadata.
struct CompiledSpine {
    /// The built, collision-checked `VirtualSpine`.
    spine: VirtualSpine,
    /// The compiled bundle's flattened path→bytes map (documents and assets
    /// together; `assets` distinguishes them).
    files: typst_bundle::VirtualFs,
    /// Output paths in `files` that are raw assets rather than compiled documents.
    assets: HashSet<String>,
    /// The synthesized bundle main, present only when `emit_bundle_source` is set.
    bundle_source: Option<String>,
    /// Each compiled document's Typst-resolved `DocumentMeta`, keyed by the
    /// same output-path string form as `assets`.
    meta: HashMap<String, DocumentMeta>,
}

impl Build {
    /// Select formats, apply each plugin's smart defaults, and resolve the
    /// output and font directories, producing a ready-to-run [`Build`].
    ///
    /// `all_plugins` is the full set of known plugins; the build keeps only those
    /// whose names are selected by `opts.formats` (or the project config / all).
    pub fn prepare(
        mut project: ProjectConfig,
        all_plugins: Vec<Box<dyn FormatPlugin>>,
        opts: BuildOptions,
    ) -> Result<Self> {
        let formats = determine_formats(opts.formats, &project.config.formats, &all_plugins);
        let plugins = plugins_for_formats(&formats, all_plugins);

        // Apply plugin smart defaults — each plugin fills in only missing values.
        for plugin in &plugins {
            let section = project
                .config
                .plugin_sections
                .entry(plugin.name().to_string())
                .or_default();
            plugin.apply_defaults(section, &project.name);
        }

        let resolved_build_dir = resolve_build_dir(&project, opts.build_dir)?;
        let output = OutputConfig::new(&project.root, resolved_build_dir);
        let font_dirs = resolve_font_dirs(&project, &opts.font_dirs);

        Ok(Self {
            project,
            plugins,
            output,
            font_dirs,
            emit_bundle_source: opts.emit_bundle_source,
        })
    }

    /// The project being built.
    pub fn project(&self) -> &ProjectConfig {
        &self.project
    }

    /// The format plugins selected for this build.
    pub fn plugins(&self) -> &[Box<dyn FormatPlugin>] {
        &self.plugins
    }

    /// The resolved output configuration (build directory layout).
    pub fn output_config(&self) -> &OutputConfig {
        &self.output
    }

    /// Derive the set of asset sources the watcher should treat as relevant.
    ///
    /// Mirrors the asset-resolution pass in [`run`](Self::run) across every
    /// enabled plugin — collecting each resolved asset's source path, the
    /// project/plugin/package `copy` glob patterns, and the source roots of any
    /// packages that declare assets — so watch coverage is driven by what the
    /// plugins actually declare rather than a hard-coded extension list.
    ///
    /// Only packages that declare rheo assets in their `typst.toml` contribute a
    /// source root here, so the watched set stays tight (immutable `@preview`
    /// deps that declare nothing are never watched).
    pub fn watch_asset_spec(&self) -> crate::assets::watch::WatchAssetSpec {
        let default_section = PluginSection::default();
        let package_imports = crate::plugins::scan_project_package_imports(&self.project.typ_files);

        let mut asset_paths: Vec<PathBuf> = Vec::new();
        let mut copy_globs: Vec<(PathBuf, Vec<String>)> = Vec::new();
        let mut package_roots: Vec<PathBuf> = Vec::new();

        for plugin in &self.plugins {
            let plugin_output_dir = self.output.dir_for_plugin(plugin.name());
            let plugin_section: &PluginSection = self
                .project
                .config
                .plugin_sections
                .get(plugin.name())
                .unwrap_or(&default_section);

            let manifest_blocks =
                manifest_blocks_for(&package_imports, plugin_section, plugin.name());

            let resolver = AssetResolver::new(&self.project.root, &plugin_output_dir);
            if let Ok(resolved) =
                resolver.resolve(plugin.as_ref(), plugin_section, &manifest_blocks)
            {
                for assets in resolved.values() {
                    for asset in assets {
                        asset_paths.push(asset.source_path.clone());
                    }
                }
            }

            // Copy globs: project-level, per-package, and per-plugin asset blocks.
            if !self.project.config.copy.is_empty() {
                copy_globs.push((self.project.root.clone(), self.project.config.copy.clone()));
            }
            for block in &manifest_blocks {
                if !block.assets.copy.is_empty() {
                    copy_globs.push((block.source_root.clone(), block.assets.copy.clone()));
                }
                package_roots.push(block.source_root.clone());
            }
            for block in plugin_section.asset_blocks() {
                if !block.copy.is_empty() {
                    copy_globs.push((self.project.root.clone(), block.copy.clone()));
                }
            }
        }

        crate::assets::watch::WatchAssetSpec::new(asset_paths, copy_globs, package_roots)
    }

    /// Build the virtual spine for `plugin` and compile it to an in-memory bundle.
    ///
    /// Shared by the full build and the dev-server watch path: resolves the spine
    /// options, generates the spine files for the project mode, builds and
    /// collision-checks the `VirtualSpine`, then moulds it and compiles the Typst
    /// bundle into a `VirtualFs`. Returns a [`CompiledSpine`] bundling:
    ///
    /// - `spine` / `files`: the built `VirtualSpine` and the compiled bundle's
    ///   flattened path→bytes map.
    /// - `assets`: the set of output paths that are raw *assets* rather than
    ///   compiled documents. Export flattens both kinds into one path→bytes map,
    ///   so this set is the only surviving record of which entries must bypass
    ///   the format plugin and be written verbatim.
    /// - `bundle_source`: the synthesized bundle main, present only when
    ///   `self.emit_bundle_source` is set (the caller writes it to
    ///   `.rheo-bundle.typ`; `compile_spine` has no `plugin_output_dir`).
    /// - `meta`: each compiled document's Typst-resolved `DocumentMeta`, keyed
    ///   by the same output-path string form as `assets`.
    fn compile_spine(
        &self,
        plugin: &dyn FormatPlugin,
        plugin_section: &PluginSection,
        content_dir: &Path,
    ) -> Result<CompiledSpine> {
        // Effective spine config: each field on a per-format [plugin.spine]
        // falls back to the global [spine] independently when unset — e.g.
        // `[pdf.spine] title` alone still inherits the global `exclude`,
        // rather than the per-format table's mere presence blanking every
        // global spine key at once.
        let plugin_spine = plugin_section.spine.as_ref();
        let global_spine = self.project.config.spine.as_ref();
        let exclude = plugin_spine
            .and_then(|s| s.exclude.clone())
            .or_else(|| global_spine.and_then(|s| s.exclude.clone()))
            .unwrap_or_default();
        let sections = plugin_spine
            .and_then(|s| s.section.clone())
            .or_else(|| global_spine.and_then(|s| s.section.clone()))
            .unwrap_or_default();
        let title = plugin_spine
            .and_then(|s| s.title.clone())
            .or_else(|| global_spine.and_then(|s| s.title.clone()));

        let layout = spine_layout_for(plugin.spine_layout_kind(), plugin, &self.project.name);

        // Base spine = directory scan under content_dir, customized by the two
        // knobs (exclude, then section layering). A single-file project is a
        // one-node flat spine.
        let scan = match self.project.mode {
            ProjectMode::SingleFile => {
                SpineScan::flat(&[self.project.typ_files[0].clone()], content_dir)
            }
            ProjectMode::Directory => SpineScan::run_with_marrow(
                content_dir,
                &exclude,
                self.project.config.marrow_file(),
            )?
            .apply_sections(content_dir, &sections)?,
        };

        debug!(
            plugin = plugin.name(),
            files = scan.files.len(),
            "building virtual spine"
        );

        // `ext` rides on rheo-context alongside `target` (present for per-page
        // plugin formats, omitted for the combined PDF) so typ/rheo.typ can build
        // cross-vertebra hrefs without hardcoding the extension.
        let target = plugin.rheo_target();
        let ext = target.map(|_| plugin.extension());

        // Marrow only makes sense for per-page targets: `document()` and
        // `asset()` both hard-error under the combined PDF target ("setting the
        // document format is only supported in the bundle target"), so the same
        // `ext` gate that marks a per-page format decides whether to emit it.
        let mut marrow = Vec::new();
        if ext.is_some() {
            // Imported packages contribute their own marrow first, in import
            // order, so the project's own file is spliced last and can build on
            // what they registered. Behind the same opt-out that governs every
            // other package-driven behaviour.
            if plugin_section.auto_detect_packages_enabled() {
                let package_imports =
                    crate::plugins::scan_project_package_imports(&self.project.typ_files);
                marrow.extend(crate::plugins::detect_package_marrow(&package_imports));
            }

            let marrow_path = content_dir.join(self.project.config.marrow_file());
            match std::fs::read_to_string(&marrow_path) {
                Ok(text) => marrow.push(text),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(RheoError::io(
                        e,
                        format!("reading marrow file '{}'", marrow_path.display()),
                    ));
                }
            }
        }

        let virtual_spine = VirtualSpine::build(scan, &self.project.root, layout)?
            .with_title(title)
            .with_marrow(marrow);
        virtual_spine.check_output_collisions()?;

        let moulded = virtual_spine.mould();
        let bundle_source = self.emit_bundle_source.then(|| moulded.main.clone());
        let rheo_context = virtual_spine.vertebra_injections();
        // Per-format footnote-reset toggle (default true); only takes effect for
        // per-page formats, since rheo.typ ANDs it with the `ext` gate.
        let reset_footnotes = self
            .project
            .config
            .plugin_section(plugin.name())
            .reset_footnotes();

        // Single Typst bundle compile for this plugin.
        let world = RheoWorld::new_for_bundle(
            &self.project.root,
            moulded.main,
            moulded.sources,
            rheo_context,
            Some(virtual_spine.global_context(target, ext, reset_footnotes)),
            plugin.rheo_target(),
            self.font_dirs.clone(),
        )?;
        let bundle = world.compile_bundle()?;
        // Record which entries are assets, and snapshot each document's
        // Typst-resolved metadata, before export collapses the document/asset
        // distinction into plain bytes and `bundle` itself is dropped.
        let mut assets: HashSet<String> = HashSet::new();
        let mut meta: HashMap<String, DocumentMeta> = HashMap::new();
        for (path, file) in bundle.files.iter() {
            let output_path = path.get_with_slash().trim_start_matches('/').to_string();
            match file {
                typst_bundle::BundleFile::Asset(_) => {
                    assets.insert(output_path);
                }
                typst_bundle::BundleFile::Document(doc) => {
                    meta.insert(output_path, DocumentMeta::new(doc.info().clone()));
                }
            }
        }
        let virtual_fs = export_bundle(&bundle)?;

        Ok(CompiledSpine {
            spine: virtual_spine,
            files: virtual_fs,
            assets,
            bundle_source,
            meta,
        })
    }

    /// Compile HTML to an in-memory VirtualFs for the dev server.
    ///
    /// Resolves assets (CSS/JS), copies them to the plugin output dir so the
    /// dev server can fall back to disk for them, compiles the Typst bundle, and
    /// injects CSS/JS `<link>`/`<script>` tags into each HTML entry before
    /// returning the VirtualFs. Typst's comemo layer caches unchanged sources, so
    /// this second compile after `run()` is near-instant on unchanged content.
    ///
    /// Returns `None` when the HTML plugin is not selected.
    pub fn compile_for_watch(&mut self) -> Result<Option<typst_bundle::VirtualFs>> {
        if self.project.typ_files.is_empty() {
            return Err(RheoError::project_config("no .typ files found in project"));
        }

        let html_plugin = match self.plugins.iter().find(|p| p.name() == "html") {
            Some(p) => p,
            None => return Ok(None),
        };

        let plugin_output_dir = self.output.dir_for_plugin(html_plugin.name());
        ensure_output_dir(&plugin_output_dir, html_plugin.name())?;

        let default_section = PluginSection::default();

        let content_dir = resolve_effective_content_dir(&self.project);

        let plugin_section: &PluginSection = self
            .project
            .config
            .plugin_sections
            .get(html_plugin.name())
            .unwrap_or(&default_section);

        // Resolve assets — copies CSS/JS to disk so the dev server can serve them
        // as fallback for requests not satisfied by the VirtualFs.
        //
        // Mirror `run()`: detect package-provided assets (e.g. a sidebar
        // package's stylesheet and script declared in its `typst.toml`) so the
        // in-memory entry page injects the same head links as the on-disk build.
        // Without this the VirtualFs index lacks package CSS/JS and renders
        // unstyled.
        let package_imports = crate::plugins::scan_project_package_imports(&self.project.typ_files);
        prewarm_and_check_versions(&package_imports, plugin_section)?;
        let manifest_blocks =
            manifest_blocks_for(&package_imports, plugin_section, html_plugin.name());

        let resolver = AssetResolver::new(&self.project.root, &plugin_output_dir);
        let resolved_assets = resolver
            .resolve(html_plugin.as_ref(), plugin_section, &manifest_blocks)
            .unwrap_or_default();

        let css_paths: Vec<String> = resolved_assets
            .get("css_stylesheet")
            .map(|v| v.iter().map(|a| a.built_relative_path.clone()).collect())
            .unwrap_or_default();
        let js_paths: Vec<String> = resolved_assets
            .get("js_scripts")
            .map(|v| v.iter().map(|a| a.built_relative_path.clone()).collect())
            .unwrap_or_default();

        let CompiledSpine {
            files: virtual_fs,
            assets,
            ..
        } = self.compile_spine(html_plugin.as_ref(), plugin_section, &content_dir)?;

        // Build a page-path -> HTML string map from the non-asset (compiled
        // document) entries of this same virtual_fs, for `<rheo-content>`
        // placeholder resolution below. This path has no `CastVertebra`s to
        // walk (unlike `run()`, it doesn't call `flatten_bundle_outputs`), so
        // it shares only the scan/resolve logic via `ContentTransclusion`'s
        // map-based resolve variant. Entries in `assets` are excluded: those
        // are the bundle assets being transcluded INTO, not pages to
        // transclude FROM.
        let pages: HashMap<String, String> = virtual_fs
            .iter()
            .filter_map(|(vpath, bytes)| {
                let path_str = vpath.get_with_slash().trim_start_matches('/').to_string();
                if assets.contains(&path_str) || !path_str.ends_with(".html") {
                    return None;
                }
                Some((
                    path_str,
                    String::from_utf8_lossy(bytes.as_slice()).into_owned(),
                ))
            })
            .collect();

        // Scan for a `.rheo/head.html` control asset before consuming
        // `virtual_fs` below — reuses the same decode-or-error/unrecognized-warn
        // classification `ControlAssets::extract` uses on the `run()` path, so
        // an unrecognized `.rheo/*` member only ever warns in one shared place.
        let mut control_head_fragment: Option<String> = None;
        for (vpath, bytes) in virtual_fs.iter() {
            let path_str = vpath.get_with_slash().trim_start_matches('/').to_string();
            if let ControlAssetKind::HeadFragment(text) =
                ControlAssets::classify_asset(&path_str, bytes)?
            {
                control_head_fragment = Some(text);
            }
        }

        // Inject CSS/JS link tags into each HTML entry in memory, and rewrite
        // `<rheo-content>` placeholders in bundle-emitted assets, so `rheo
        // watch` serves the same transcluded bytes `rheo compile` writes to
        // disk. Assets stay in the returned VirtualFs so the dev server still
        // serves them. `.rheo/*` control assets are dropped entirely — never
        // served — mirroring the `run()` path's `ControlAssets::extract`.
        let needs_injection = !css_paths.is_empty() || !js_paths.is_empty();
        let needs_head_mutation = needs_injection || control_head_fragment.is_some();
        let mut injected = typst_bundle::VirtualFs::default();
        for (vpath, bytes) in virtual_fs {
            let path_str = vpath.get_with_slash().trim_start_matches('/').to_string();

            if ControlAssets::is_control_asset(&path_str) {
                continue;
            }

            if assets.contains(&path_str) {
                let bytes = match std::str::from_utf8(bytes.as_slice()) {
                    Ok(text) => {
                        match ContentTransclusion::rewrite_from_map(&path_str, text, &pages)? {
                            Some(rewritten) => {
                                typst::foundations::Bytes::new(rewritten.into_bytes())
                            }
                            None => bytes,
                        }
                    }
                    Err(_) => bytes,
                };
                injected.insert(vpath, bytes);
                continue;
            }

            if needs_head_mutation && path_str.ends_with(".html") {
                let html = String::from_utf8_lossy(&bytes);
                let mut dom = crate::util::html::HtmlDom::parse(&html)?;
                if needs_injection {
                    // Depth-relative asset refs so nested pages resolve them.
                    let prefix = crate::util::html::depth_prefix(&path_str);
                    let css_refs: Vec<String> =
                        css_paths.iter().map(|s| format!("{prefix}{s}")).collect();
                    let js_refs: Vec<String> =
                        js_paths.iter().map(|s| format!("{prefix}{s}")).collect();
                    let css: Vec<&str> = css_refs.iter().map(|s| s.as_str()).collect();
                    let js: Vec<&str> = js_refs.iter().map(|s| s.as_str()).collect();
                    dom.inject_head_links(&[], &css, &js)?;
                }
                if let Some(fragment) = &control_head_fragment {
                    dom.append_head_fragment(fragment)?;
                }
                let modified = dom.serialize()?;
                injected.insert(vpath, typst::foundations::Bytes::new(modified.into_bytes()));
            } else {
                injected.insert(vpath, bytes);
            }
        }
        Ok(Some(injected))
    }

    /// Compile the project across all selected plugins.
    ///
    /// Returns the per-format [`CompilationResults`] on full success. If any
    /// format fails, the failure is logged and an error is returned (the CLI maps
    /// this to a non-zero exit).
    pub fn run(&mut self) -> Result<CompilationResults> {
        if self.project.typ_files.is_empty() {
            return Err(RheoError::project_config("no .typ files found in project"));
        }

        let mut results = CompilationResults::new();
        let default_section = PluginSection::default();

        // Scan .typ files for package imports once — shared across all plugins.
        let package_imports = crate::plugins::scan_project_package_imports(&self.project.typ_files);

        let content_dir = resolve_effective_content_dir(&self.project);

        for plugin in &self.plugins {
            let plugin_output_dir = self.output.dir_for_plugin(plugin.name());
            ensure_output_dir(&plugin_output_dir, plugin.name())?;

            let plugin_section: &PluginSection = self
                .project
                .config
                .plugin_sections
                .get(plugin.name())
                .unwrap_or(&default_section);

            prewarm_and_check_versions(&package_imports, plugin_section)?;
            let manifest_blocks =
                manifest_blocks_for(&package_imports, plugin_section, plugin.name());

            let resolver = AssetResolver::new(&self.project.root, &plugin_output_dir);
            let resolved_assets =
                resolver.resolve(plugin.as_ref(), plugin_section, &manifest_blocks)?;

            let CompiledSpine {
                spine: virtual_spine,
                files: virtual_fs,
                assets,
                bundle_source,
                meta,
            } = self.compile_spine(plugin.as_ref(), plugin_section, &content_dir)?;

            // Read-only debug artifact — never read back as an input. Written
            // under the plugin's build-dir output, which the watcher already
            // excludes wholesale, so this cannot self-trigger a rebuild loop.
            if let Some(source) = bundle_source {
                let debug_path = plugin_output_dir.join(".rheo-bundle.typ");
                std::fs::write(&debug_path, source).map_err(|e| {
                    RheoError::io(
                        e,
                        format!("writing bundle debug source to {}", debug_path.display()),
                    )
                })?;
            }

            let (outputs, mut asset_files) = flatten_bundle_outputs(
                virtual_fs,
                &assets,
                &virtual_spine,
                plugin.typst_format(),
                &meta,
            );

            // Resolve any `<rheo-content>` placeholders bundle-emitted assets
            // (e.g. a marrow-minted Atom feed) contain, embedding the compiled
            // inner HTML of the pages they name. Runs before the
            // `embeds_bundle_assets` branch below so both loose-file HTML
            // assets and EPUB-embedded assets (built from this same
            // `asset_files`) get transcluded bytes. A no-op (byte-identical)
            // for an asset with no placeholder or non-UTF-8 bytes.
            ContentTransclusion::rewrite_assets(&outputs, &mut asset_files)?;

            // Pull bundle-root control assets (`.rheo/*`) out of the plugin
            // asset list before the plugin — or the EPUB embedding path,
            // which embeds straight from this same `asset_files` — ever sees
            // them. A no-op when the project has no `.rheo/*` assets.
            let (asset_files, control) = ControlAssets::extract(asset_files)?;

            debug!(
                plugin = plugin.name(),
                outputs = outputs.len(),
                assets = asset_files.len(),
                "spine compile produced outputs"
            );

            let ctx = PluginContext {
                project: &self.project,
                output_dir: &plugin_output_dir,
                spine: &virtual_spine,
                config: plugin_section,
                assets: &resolved_assets,
                font_dirs: &self.font_dirs,
                bundle_assets: &asset_files,
                control: &control,
            };

            // Assets are the lowest precedence tier — asset() < spine documents
            // < copy globs — so they land before the plugin writes its pages and
            // long before `copy_globs` runs below. A plugin that embeds bundle
            // assets itself (e.g. EPUB, via `ctx.bundle_assets`) takes over
            // placing them instead — a loose file next to a packaged container
            // would be unreachable from inside it.
            if !plugin.embeds_bundle_assets() {
                for (path, bytes) in &asset_files {
                    let dest = plugin_output_dir.join(path);
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            RheoError::io(e, format!("creating directory for asset {path}"))
                        })?;
                    }
                    std::fs::write(&dest, bytes.as_slice())
                        .map_err(|e| RheoError::io(e, format!("writing asset {path}")))?;
                }
            }

            match plugin.compile(ctx, &outputs) {
                Ok(_) => {
                    // Apply copy globs after bundle output is written so that
                    // explicit copy patterns win over any colliding bundle output.
                    resolver.copy_globs(
                        &self.project.config.copy,
                        &self.project.root,
                        None,
                        true,
                    )?;
                    for block in &manifest_blocks {
                        resolver.copy_globs(
                            &block.assets.copy,
                            &block.source_root,
                            block.assets.dest.as_deref(),
                            true,
                        )?;
                    }
                    for block in plugin_section.asset_blocks() {
                        resolver.copy_globs(
                            &block.copy,
                            &self.project.root,
                            block.dest.as_deref(),
                            true,
                        )?;
                    }
                    results.record_success(plugin.name());
                    info!(plugin = plugin.name(), "compilation succeeded");
                }
                Err(e) => {
                    error!(error = %e, "{} generation failed", plugin.name());
                    results.record_failure(plugin.name());
                }
            }
        }

        let names: Vec<&str> = self.plugins.iter().map(|p| p.name()).collect();
        results.log_summary(&names);

        if results.has_failures() {
            if names.iter().any(|name| results.get(name).succeeded > 0) {
                return Err(RheoError::project_config(
                    "some formats failed to compile".to_string(),
                ));
            }
            return Err(RheoError::project_config(
                "all formats failed or no files were compiled".to_string(),
            ));
        }

        info!("compilation complete");
        Ok(results)
    }
}

/// Split a compiled bundle's flat path→bytes map into plugin-facing documents
/// and raw assets.
///
/// `VirtualPath::get_with_slash()` gives the path string (e.g. `"/intro.html"`);
/// the leading `/` is stripped to produce a relative filename. Each document's
/// title/date/description/keywords/author are read from its Typst-resolved
/// `DocumentMeta` in `meta` (Typst's own realization — see
/// [`Build::compile_spine`]), falling back to the matching spine `Vertebra`'s
/// purely path-derived title when no `DocumentMeta` exists for the output
/// (shouldn't normally happen for a real document, but keeps nothing blank);
/// `date` has no such fallback, since `Vertebra` no longer carries one.
/// The matching `Vertebra` is still used to detect `contributed` outputs (no
/// matching vertebra at all, e.g. a combined output or a marrow contribution).
/// Paths in `assets` came from an `asset()` element rather than a `document()`
/// one, so they are returned separately to be written verbatim — handing them
/// to the format plugin would treat raw bytes as a page.
fn flatten_bundle_outputs(
    virtual_fs: typst_bundle::VirtualFs,
    assets: &HashSet<String>,
    spine: &VirtualSpine,
    format: TypstFormat,
    meta: &HashMap<String, DocumentMeta>,
) -> (Vec<CastVertebra>, Vec<(String, typst::foundations::Bytes)>) {
    let mut documents = Vec::new();
    let mut asset_files = Vec::new();

    for (vpath, bytes) in virtual_fs {
        let output_path = vpath.get_with_slash().trim_start_matches('/').to_string();
        if assets.contains(&output_path) {
            asset_files.push((output_path, bytes));
            continue;
        }
        let vertebra = spine
            .vertebrae
            .iter()
            .find(|v| v.output_path == output_path);
        let doc_meta = meta.get(&output_path);
        documents.push(CastVertebra {
            title: doc_meta
                .and_then(DocumentMeta::title)
                .map(str::to_string)
                .or_else(|| vertebra.map(|v| v.title.clone()))
                .unwrap_or_default(),
            date: doc_meta.and_then(DocumentMeta::date),
            description: doc_meta
                .and_then(DocumentMeta::description)
                .map(str::to_string),
            keywords: doc_meta
                .map(|m| {
                    m.keywords()
                        .iter()
                        .map(|k| k.to_string())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
            author: doc_meta
                .map(|m| {
                    m.author()
                        .iter()
                        .map(|a| a.to_string())
                        .collect::<Vec<String>>()
                })
                .unwrap_or_default(),
            output_path,
            bytes,
            format,
            contributed: vertebra.is_none(),
        });
    }

    (documents, asset_files)
}

/// Determine which format names to compile.
///
/// Priority: explicit `enabled` list → config `formats` → all plugins.
fn determine_formats(
    enabled: Vec<String>,
    config_defaults: &[String],
    all: &[Box<dyn FormatPlugin>],
) -> Vec<String> {
    if !enabled.is_empty() {
        return enabled;
    }
    if !config_defaults.is_empty() {
        return config_defaults.to_vec();
    }
    all.iter().map(|p| p.name().to_string()).collect()
}

/// Keep only the plugins whose names appear in `formats`.
fn plugins_for_formats(
    formats: &[String],
    all: Vec<Box<dyn FormatPlugin>>,
) -> Vec<Box<dyn FormatPlugin>> {
    all.into_iter()
        .filter(|p| formats.iter().any(|f| f == p.name()))
        .collect()
}

/// Create `dir` if missing, naming `plugin_name` in any IO error.
fn ensure_output_dir(dir: &Path, plugin_name: &str) -> Result<()> {
    std::fs::create_dir_all(dir)
        .map_err(|e| RheoError::io(e, format!("creating output directory for {plugin_name}")))
}

/// Pre-warms `package_imports` (if enabled) and rejects any whose declared
/// `[tool.rheo] min_version` exceeds this build, before assets are detected.
fn prewarm_and_check_versions(
    package_imports: &[String],
    plugin_section: &PluginSection,
) -> Result<()> {
    if plugin_section.auto_detect_packages_enabled() {
        crate::plugins::prewarm_packages(package_imports);
    }
    crate::plugins::check_package_min_versions(package_imports)
}

/// Auto-detected manifest asset blocks for `plugin_section`'s format, or
/// none when the section disables package auto-detection.
fn manifest_blocks_for(
    package_imports: &[String],
    plugin_section: &PluginSection,
    format_name: &str,
) -> Vec<crate::plugins::PackageAssets> {
    if plugin_section.auto_detect_packages_enabled() {
        crate::plugins::detect_manifest_package_assets(package_imports, format_name)
    } else {
        vec![]
    }
}

/// Resolve a path relative to a base directory (absolute paths pass through).
fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

/// Resolve the build directory with priority: CLI arg > config > default (`None`).
pub fn resolve_build_dir(
    project: &ProjectConfig,
    cli_build_dir: Option<PathBuf>,
) -> Result<Option<PathBuf>> {
    if let Some(cli_path) = cli_build_dir {
        let cwd =
            std::env::current_dir().map_err(|e| RheoError::io(e, "getting current directory"))?;
        debug!(dir = %cli_path.display(), "build directory");
        Ok(Some(resolve_path(&cwd, &cli_path)))
    } else if let Some(config_path) = &project.config.build_dir {
        let resolved = resolve_path(&project.root, Path::new(config_path));
        debug!(dir = %resolved.display(), "build directory");
        Ok(Some(resolved))
    } else {
        Ok(None)
    }
}

/// Resolve the effective content directory for a project.
///
/// If `content_dir` is set in config, use it. Otherwise, if `project.root/content/`
/// exists and all `.typ` files in the project are under it, treat it as the implicit
/// content root so that file stems are relative to `content/` (not the project root).
pub fn resolve_effective_content_dir(project: &ProjectConfig) -> PathBuf {
    if let Some(dir) = project.config.resolve_content_dir(&project.root) {
        return dir;
    }
    let candidate = project.root.join("content");
    if candidate.is_dir()
        && !project.typ_files.is_empty()
        && project.typ_files.iter().all(|f| f.starts_with(&candidate))
    {
        debug!(content_dir = %candidate.display(), "auto-detected content directory");
        return candidate;
    }
    project.root.clone()
}

/// Resolve font directories with autoscan, config, and CLI precedence.
fn resolve_font_dirs(project: &ProjectConfig, cli_font_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if project.config.font_dirs.is_empty() {
        let autoscan_dir = project.root.join("fonts");
        if autoscan_dir.is_dir() {
            debug!(dir = %autoscan_dir.display(), "auto-discovered font directory");
            dirs.push(autoscan_dir);
        }
    } else {
        dirs.extend(project.config.resolve_font_dirs(&project.root));
    }

    let cwd = std::env::current_dir().unwrap();
    for dir in cli_font_dirs {
        dirs.push(resolve_path(&cwd, dir));
    }

    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_names(all: &[Box<dyn FormatPlugin>]) -> Vec<String> {
        all.iter().map(|p| p.name().to_string()).collect()
    }

    struct FakePlugin(&'static str);
    impl FormatPlugin for FakePlugin {
        fn name(&self) -> &'static str {
            self.0
        }
        fn compile(&self, _ctx: PluginContext<'_>, _outputs: &[CastVertebra]) -> crate::Result<()> {
            Ok(())
        }
    }

    fn fake_all() -> Vec<Box<dyn FormatPlugin>> {
        vec![
            Box::new(FakePlugin("html")),
            Box::new(FakePlugin("pdf")),
            Box::new(FakePlugin("epub")),
        ]
    }

    /// A bundle flattens documents and assets into one path→bytes map, so an
    /// `asset()` output would otherwise reach the format plugin as if it were a
    /// page. `flatten_bundle_outputs` keeps the two apart. No `DocumentMeta` is
    /// supplied for this test's output, so its title falls back to the matching
    /// `Vertebra`'s path-derived title.
    #[test]
    fn test_flatten_bundle_outputs_separates_assets_from_documents() {
        use crate::reticulate::spine::{SpineLayout, Vertebra, VirtualSpine};
        use typst::foundations::Bytes;
        use typst_syntax::VirtualPath;

        let spine = VirtualSpine {
            vertebrae: vec![Vertebra {
                rel_path: "content/index.typ".into(),
                output_path: "index.html".into(),
                handle: "index".into(),
                extra_handles: vec![],
                emit_handle: true,
                title: "Index".into(),
                source: String::new(),
            }],
            layout: SpineLayout::OnePerVertebra {
                ext: "html".into(),
                format: "html".into(),
            },
            tree: vec![],
            title: None,
            marrow: Vec::new(),
        };

        let mut virtual_fs = typst_bundle::VirtualFs::default();
        virtual_fs.insert(
            VirtualPath::new("index.html").expect("valid virtual path"),
            Bytes::new(b"<html></html>".to_vec()),
        );
        virtual_fs.insert(
            VirtualPath::new("data/x.json").expect("valid virtual path"),
            Bytes::new(b"{}".to_vec()),
        );

        let assets = HashSet::from(["data/x.json".to_string()]);
        let meta = HashMap::new();
        let (documents, asset_files) =
            flatten_bundle_outputs(virtual_fs, &assets, &spine, TypstFormat::Html, &meta);

        assert_eq!(documents.len(), 1, "the asset must not become a page");
        assert_eq!(documents[0].output_path, "index.html");
        assert_eq!(
            documents[0].title, "Index",
            "no DocumentMeta for this output falls back to vertebra's path-derived title"
        );
        assert_eq!(asset_files.len(), 1);
        assert_eq!(asset_files[0].0, "data/x.json");
        assert_eq!(asset_files[0].1.as_slice(), b"{}");
    }

    /// When a `DocumentMeta` exists for an output, it wins over the matching
    /// `Vertebra`'s path-derived title (and there is no `date` fallback at
    /// all) — the Typst-resolved value is the real authored one, which can
    /// differ from a filename-derived guess (e.g. a title set via an
    /// imported `#show:` template). The description/keywords/author fields
    /// come from `DocumentMeta` alone.
    #[test]
    fn test_flatten_bundle_outputs_prefers_document_meta_over_vertebra() {
        use crate::reticulate::spine::{SpineLayout, Vertebra, VirtualSpine};
        use typst::foundations::Bytes;
        use typst::model::DocumentInfo;
        use typst_syntax::VirtualPath;

        let spine = VirtualSpine {
            vertebrae: vec![Vertebra {
                rel_path: "content/index.typ".into(),
                output_path: "index.html".into(),
                handle: "index".into(),
                extra_handles: vec![],
                emit_handle: true,
                title: "Fallback Title".into(),
                source: String::new(),
            }],
            layout: SpineLayout::OnePerVertebra {
                ext: "html".into(),
                format: "html".into(),
            },
            tree: vec![],
            title: None,
            marrow: Vec::new(),
        };

        let mut virtual_fs = typst_bundle::VirtualFs::default();
        virtual_fs.insert(
            VirtualPath::new("index.html").expect("valid virtual path"),
            Bytes::new(b"<html></html>".to_vec()),
        );

        let info = DocumentInfo {
            title: Some("Real Title".into()),
            author: vec!["Ada Lovelace".into()],
            description: Some("A description".into()),
            keywords: vec!["foo".into()],
            ..Default::default()
        };
        let meta = HashMap::from([("index.html".to_string(), DocumentMeta::new(info))]);

        let (documents, _asset_files) = flatten_bundle_outputs(
            virtual_fs,
            &HashSet::new(),
            &spine,
            TypstFormat::Html,
            &meta,
        );

        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].title, "Real Title");
        assert_eq!(documents[0].author, vec!["Ada Lovelace".to_string()]);
        assert_eq!(documents[0].description.as_deref(), Some("A description"));
        assert_eq!(documents[0].keywords, vec!["foo".to_string()]);
    }

    /// End-to-end: a vertebra whose title is set only via an imported
    /// `#show: book` template (the shape `docs/limitations.md` and
    /// `../rheo-tests/cases/metadata_template_title` describe) gets its real
    /// authored title through the full `Build::run` pipeline — not the
    /// filename-derived fallback the pre-compile AST scan is stuck with.
    #[test]
    fn test_run_resolves_title_from_imported_template_via_document_info() {
        use crate::config::project::ProjectConfig;
        use std::sync::{Arc, Mutex};

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        std::fs::write(
            root.join("template.typ"),
            "#let book(doc) = {\n  set document(title: [Templated Title From Book])\n  doc\n}\n",
        )
        .expect("write template.typ");
        std::fs::create_dir_all(root.join("content")).expect("create content dir");
        std::fs::write(
            root.join("content/templated.typ"),
            "#import \"/template.typ\": book\n#show: book\n\n= Templated Chapter\n",
        )
        .expect("write content/templated.typ");

        let project = ProjectConfig {
            name: "test".to_string(),
            root: root.to_path_buf(),
            config: crate::RheoConfig {
                content_dir: Some("content".to_string()),
                formats: vec!["html".to_string()],
                ..Default::default()
            },
            typ_files: vec![root.join("content/templated.typ")],
            mode: ProjectMode::Directory,
            config_path: None,
        };

        #[derive(Default)]
        struct Captured(Mutex<Vec<(String, String)>>);

        struct CapturingPlugin(Arc<Captured>);
        impl FormatPlugin for CapturingPlugin {
            fn name(&self) -> &'static str {
                "html"
            }
            fn compile(&self, _ctx: PluginContext<'_>, outputs: &[CastVertebra]) -> Result<()> {
                let mut guard = self.0.0.lock().unwrap();
                for o in outputs {
                    guard.push((o.output_path.clone(), o.title.clone()));
                }
                Ok(())
            }
        }

        let captured = Arc::new(Captured::default());
        let plugin: Box<dyn FormatPlugin> = Box::new(CapturingPlugin(captured.clone()));

        let mut build =
            Build::prepare(project, vec![plugin], BuildOptions::default()).expect("prepare build");
        build.run().expect("run build");

        let outputs = captured.0.lock().unwrap();
        let (_, title) = outputs
            .iter()
            .find(|(path, _)| path == "templated.html")
            .expect("templated.html output present");
        assert_eq!(
            title, "Templated Title From Book",
            "CastVertebra.title should be Typst's resolved DocumentInfo.title, \
             not the AST scan's filename-derived fallback"
        );
    }

    #[test]
    fn test_determine_formats_cli_flags_override_config() {
        let all = fake_all();
        let formats = determine_formats(vec!["pdf".into()], &["html".into()], &all);
        assert_eq!(formats, vec!["pdf".to_string()]);
    }

    #[test]
    fn test_determine_formats_uses_config_defaults_when_no_flags() {
        let all = fake_all();
        let formats = determine_formats(vec![], &["html".into()], &all);
        assert_eq!(formats, vec!["html".to_string()]);
    }

    #[test]
    fn test_determine_formats_falls_back_to_all_when_empty() {
        let all = fake_all();
        let formats = determine_formats(vec![], &[], &all);
        assert_eq!(formats, plugin_names(&all));
    }

    #[test]
    fn test_plugins_for_formats_filters() {
        let all = fake_all();
        let selected = plugins_for_formats(&["pdf".into(), "epub".into()], all);
        let names: Vec<&str> = selected.iter().map(|p| p.name()).collect();
        assert_eq!(names, vec!["pdf", "epub"]);
    }
}

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
use crate::packages::PackageIndex;
use crate::plugins::{CastVertebra, FormatPlugin, PluginContext, TypstFormat, spine_layout_for};
use crate::reticulate::document_meta::DocumentMeta;
use crate::reticulate::handle::Handle;
use crate::reticulate::spine::{FormatContext, SpineLayout, SpineScan, VirtualSpine};
use crate::transclude::{ContentTransclusion, ControlAssetKind, ControlAssets};
use crate::world::RheoWorld;
use crate::{Result, RheoError};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info};
use typst::introspection::Introspector as _;
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
    /// `--input KEY=VALUE` pairs, seeded onto `sys.inputs` for the Typst compile.
    /// Typst has no environment access, so this is the only way a build script can
    /// parameterise a compile. Values are strings, always. A key equal to
    /// `world::RESERVED_INPUT_KEY` (`rheo-context`) is rejected by the CLI and
    /// ignored here.
    pub inputs: HashMap<String, String>,
    /// `--emit-bundle-source`: write each plugin's synthesized bundle main to
    /// `<build_dir>/<plugin>/.rheo-bundle.typ`. A read-only debug artifact —
    /// never read back — for diagnosing marrow/spine authoring errors. Off by
    /// default.
    pub emit_bundle_source: bool,
    /// `--metadata-two-pass`: opt in to gated two-pass metadata resolution
    /// (see [`Build::compile_bundle_once`]) — recovers a title set inside a
    /// bounded code block for cross-vertebra reads (`metadata-of`, `@handle`)
    /// at the cost of a second bundle compile, and only when the first pass
    /// actually found such a gap. Off by default: the ordinary single-pass
    /// build never pays this cost.
    pub metadata_two_pass: bool,
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
    inputs: HashMap<String, String>,
    emit_bundle_source: bool,
    metadata_two_pass: bool,
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

/// The result of one [`Build::compile_bundle_once`] call: a single Typst
/// bundle compile, decomposed into assets/metadata/exported files, plus the
/// `bundle` itself — kept around (rather than dropped once exported) so the
/// gated second pass can query its introspector for each vertebra's own
/// beacon-reported title.
struct CompiledBundlePass {
    assets: HashSet<String>,
    meta: HashMap<String, DocumentMeta>,
    files: typst_bundle::VirtualFs,
    bundle: typst_bundle::Bundle,
}

/// The result of [`Build::resolve_spine_scan`]: the merged spine config's
/// directory-scan output, the per-plugin spine layout, and the resolved
/// spine title — everything [`Build::build_virtual_spine`] needs alongside
/// the marrow gathered by [`Build::resolve_marrow`].
struct SpineScanResult {
    scan: SpineScan,
    layout: SpineLayout,
    title: Option<String>,
}

/// The result of [`Build::resolve_marrow`]: the per-plugin output target and
/// extension, and the marrow contributions gathered for it.
struct MarrowContext {
    target: Option<&'static str>,
    ext: Option<&'static str>,
    marrow: Vec<String>,
    marrow_prologue: Vec<String>,
}

/// The result of [`Build::mould_bundle`]: the synthesized bundle main and
/// source overlay, the per-vertebra `rheo-context` injections, the optional
/// debug bundle source, and the resolved footnote-reset toggle — everything
/// [`Build::compile_bundle_once`] needs beyond the spine itself.
///
/// `main`/`sources`/`rheo_context` are consumed by clone in `compile_spine`'s
/// first pass rather than moved, so the originals survive for the gated
/// second pass.
struct MouldedBundle {
    main: String,
    sources: HashMap<String, String>,
    rheo_context: HashMap<String, crate::reticulate::VertebraInjection>,
    bundle_source: Option<String>,
    reset_footnotes: bool,
}

/// One plugin's resolved asset context — its output directory, its config
/// section, the auto-detected package asset blocks, and its resolved assets.
///
/// The prologue that produces this is the part [`Build::run`],
/// [`Build::compile_for_watch`] and [`Build::watch_asset_spec`] share before
/// they diverge; each used to open-code it, and each handled a resolve failure
/// differently.
struct PluginAssetContext<'a> {
    output_dir: PathBuf,
    section: &'a PluginSection,
    manifest_blocks: Vec<crate::plugins::PackageAssets>,
    resolved: HashMap<&'static str, Vec<crate::plugins::Asset>>,
}

/// The result of [`Build::compile_plugin_spine`]: one plugin's resolved asset
/// context alongside its compiled spine — the "compile a spine for one
/// plugin" shape [`Build::run`] and [`Build::compile_for_watch`] share, up to
/// the point where their post-processing diverges (`run` flattens into pages
/// for the format plugin; `compile_for_watch` injects CSS/JS and
/// transclusion for the dev server's in-memory VirtualFs).
struct PluginCompile<'a> {
    ctx: PluginAssetContext<'a>,
    spine: CompiledSpine,
}

/// The result of [`Build::prepare_plugin_run`]: one plugin's compiled,
/// flattened, transclusion-resolved outputs, ready to build a
/// [`PluginContext`] and hand to [`FormatPlugin::compile`].
struct PluginRunInputs<'a> {
    output_dir: PathBuf,
    spine: VirtualSpine,
    section: &'a PluginSection,
    resolved: HashMap<&'static str, Vec<crate::plugins::Asset>>,
    manifest_blocks: Vec<crate::plugins::PackageAssets>,
    outputs: Vec<CastVertebra>,
    asset_files: Vec<(String, typst::foundations::Bytes)>,
    control: ControlAssets,
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
        let font_dirs = resolve_font_dirs(&project, &opts.font_dirs)?;
        // THE ONE MERGE SITE for the two `sys.inputs` sources, so they cannot
        // disagree about precedence. `rheo.toml [inputs]` is the BASE — a project
        // declares its defaults there and the ordinary build needs no flags — and
        // each `--input KEY=VALUE` OVERRIDES that key, because a flag is the more
        // specific statement of intent. Keys the config sets and the CLI does not
        // survive untouched, so overriding one input does not clear the rest.
        //
        // Both sources reject `world::RESERVED_INPUT_KEY` before reaching here
        // (the CLI in `parse_inputs`, the config in its `TryFrom`), and
        // `build_inputs` refuses it a third time — a library caller can construct
        // `WorldSpec` directly, so the last line of defence lives there.
        let mut inputs = project.config.inputs.clone();
        inputs.extend(opts.inputs);

        Ok(Self {
            project,
            plugins,
            output,
            font_dirs,
            inputs,
            emit_bundle_source: opts.emit_bundle_source,
            metadata_two_pass: opts.metadata_two_pass,
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
    /// source root here through the per-plugin pass below, so the watched set
    /// stays tight (immutable `@preview` deps that declare nothing are never
    /// watched) — EXCEPT a `path`-backed package, added unconditionally just
    /// after: unlike a repository ref or a release, both immutable caches, its
    /// tree can change while the watch is running, so it must be covered
    /// whether or not it ships an asset block.
    pub fn watch_asset_spec(&self) -> crate::assets::watch::WatchAssetSpec {
        let default_section = PluginSection::default();
        let resolver = self.package_resolver();
        let packages = PackageIndex::resolved(
            &crate::packages::scan_project_package_imports(&self.project.typ_files),
            &resolver,
        );

        let mut asset_paths: Vec<PathBuf> = Vec::new();
        let mut copy_globs: Vec<crate::assets::CopyGlobs> = Vec::new();
        // Canonicalized so a compiled pattern's base compares equal to the
        // (canonicalized) paths the filesystem watcher reports.
        let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        let mut package_roots: Vec<PathBuf> = packages
            .source_roots()
            .filter(|(namespace, _)| resolver.is_path_backed(namespace))
            .map(|(_, root)| root.to_path_buf())
            .collect();

        for plugin in &self.plugins {
            // A resolve failure must not silently shrink the watched set: warn
            // and skip this plugin, so an unwatched stylesheet is at least
            // visible rather than an inexplicably dead rebuild.
            let ctx = match self.plugin_asset_context(
                plugin.as_ref(),
                &packages,
                &default_section,
                false,
            ) {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::warn!(
                        plugin = plugin.name(),
                        error = %e,
                        "could not resolve assets; this format's assets will not be watched"
                    );
                    continue;
                }
            };

            for assets in ctx.resolved.values() {
                for asset in assets {
                    asset_paths.push(asset.source_path.clone());
                }
            }

            // Copy globs: project-level, per-package, and per-plugin asset blocks.
            if !self.project.config.copy.is_empty()
                && let Some(g) = crate::assets::CopyGlobs::compile(
                    &canon(&self.project.root),
                    &self.project.config.copy,
                )
            {
                copy_globs.push(g);
            }
            for block in &ctx.manifest_blocks {
                if !block.assets.copy.is_empty()
                    && let Some(g) = crate::assets::CopyGlobs::compile(
                        &canon(&block.source_root),
                        &block.assets.copy,
                    )
                {
                    copy_globs.push(g);
                }
                package_roots.push(block.source_root.clone());
            }
            for block in ctx.section.asset_blocks() {
                if !block.copy.is_empty()
                    && let Some(g) =
                        crate::assets::CopyGlobs::compile(&canon(&self.project.root), &block.copy)
                {
                    copy_globs.push(g);
                }
            }
        }

        crate::assets::watch::WatchAssetSpec::new(asset_paths, copy_globs, package_roots)
    }

    /// Resolve `plugin`'s output directory, config section, package asset
    /// blocks and assets. `ensure_dir` creates the output directory — the
    /// watcher-spec path only reads asset paths, so it passes `false`.
    ///
    /// Callers that go on to compile must `prewarm_and_check_versions` FIRST:
    /// pre-warming has to happen before the manifest scan below, or a package
    /// Typst would only download at compile time is invisible to it.
    /// Whether any plugin in this build leaves package auto-detection on.
    /// The package resolver for one build, built from `[packages]`.
    ///
    /// Built per build rather than cached on `Build`, so a `watch` session that
    /// keeps one `Build` alive still re-resolves a branch as it advances.
    fn package_resolver(&self) -> Arc<crate::packages::PackageResolver> {
        Arc::new(crate::packages::PackageResolver::new(
            &self.project.config.packages,
        ))
    }

    fn auto_detects_packages(&self) -> bool {
        let default_section = PluginSection::default();
        self.plugins.iter().any(|plugin| {
            self.project
                .config
                .plugin_sections
                .get(plugin.name())
                .unwrap_or(&default_section)
                .auto_detect_packages_enabled()
        })
    }

    fn plugin_asset_context<'a>(
        &'a self,
        plugin: &dyn FormatPlugin,
        packages: &PackageIndex,
        default_section: &'a PluginSection,
        ensure_dir: bool,
    ) -> Result<PluginAssetContext<'a>> {
        let output_dir = self.output.dir_for_plugin(plugin.name());
        if ensure_dir {
            ensure_output_dir(&output_dir, plugin.name())?;
        }
        let section: &'a PluginSection = self
            .project
            .config
            .plugin_sections
            .get(plugin.name())
            .unwrap_or(default_section);
        let manifest_blocks = manifest_blocks_for(packages, section, plugin.name());
        let resolved = AssetResolver::new(&self.project.root, &output_dir).resolve(
            plugin,
            section,
            &manifest_blocks,
        )?;
        Ok(PluginAssetContext {
            output_dir,
            section,
            manifest_blocks,
            resolved,
        })
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
        resolver: &Arc<crate::packages::PackageResolver>,
    ) -> Result<CompiledSpine> {
        let SpineScanResult {
            scan,
            layout,
            title,
        } = self.resolve_spine_scan(plugin, plugin_section, content_dir)?;
        let marrow_ctx = self.resolve_marrow(plugin, plugin_section, content_dir, resolver)?;

        let virtual_spine = self.build_virtual_spine(
            scan,
            layout,
            title,
            marrow_ctx.marrow,
            marrow_ctx.marrow_prologue,
        )?;

        let moulded = self.mould_bundle(&virtual_spine, plugin);

        let mut pass = self.compile_bundle_once(
            plugin,
            moulded.main.clone(),
            moulded.sources.clone(),
            moulded.rheo_context.clone(),
            virtual_spine.global_context(FormatContext {
                target: marrow_ctx.target,
                ext: marrow_ctx.ext,
                reset_footnotes: moulded.reset_footnotes,
                title_overrides: &HashMap::new(),
            }),
            resolver,
        )?;

        // Gated second pass (`--metadata-two-pass`): only when opted in, and
        // only for a vertebra whose beacon actually got it wrong. A `#set
        // document(title:...)` inside a bounded code block leaves the
        // beacon's own `#context` read seeing whatever title was ambient
        // *before* the block (rheo's path-derived fallback, passed as the
        // `#document(...)` wrapper's own `title:` argument — itself a kind of
        // outer `set document(title:)` per Typst's docs on the matter) rather
        // than none, so "beacon title missing" can't be the trigger; only a
        // beacon-vs-`DocumentInfo` mismatch (both flattened to plain text,
        // side-stepping the beacon's rich-content title type) reliably
        // isolates the gap without also firing — and destructively flattening
        // rich content — for every vertebra whose beacon already resolves
        // correctly (see `docs/limitations.md`).
        if self.metadata_two_pass {
            let title_overrides = Self::title_overrides_for_mismatch(&virtual_spine, &pass);
            if !title_overrides.is_empty() {
                pass = self.compile_bundle_once(
                    plugin,
                    moulded.main,
                    moulded.sources,
                    moulded.rheo_context,
                    virtual_spine.global_context(FormatContext {
                        target: marrow_ctx.target,
                        ext: marrow_ctx.ext,
                        reset_footnotes: moulded.reset_footnotes,
                        title_overrides: &title_overrides,
                    }),
                    resolver,
                )?;
            }
        }

        Ok(CompiledSpine {
            spine: virtual_spine,
            files: pass.files,
            assets: pass.assets,
            bundle_source: moulded.bundle_source,
            meta: pass.meta,
        })
    }

    /// Merge the spine config and scan `content_dir` for `plugin`'s spine —
    /// the base spine (directory scan, or a one-node flat spine for a
    /// single-file project), customized by the three config knobs (exclude,
    /// then section layering, then flat reorder).
    fn resolve_spine_scan(
        &self,
        plugin: &dyn FormatPlugin,
        plugin_section: &PluginSection,
        content_dir: &Path,
    ) -> Result<SpineScanResult> {
        let spine = crate::config::Spine::merged_over(
            plugin_section.spine.as_ref(),
            self.project.config.spine.as_ref(),
        );

        let layout = spine_layout_for(plugin.spine_layout_kind(), plugin, &self.project.name);

        let scan = match self.project.mode {
            ProjectMode::SingleFile => {
                SpineScan::flat(&[self.project.typ_files[0].clone()], content_dir)
            }
            ProjectMode::Directory => SpineScan::run_with_marrow(
                content_dir,
                &spine.exclude,
                self.project.config.marrow_file(),
            )?
            .apply_sections(content_dir, &spine.section)?
            .apply_include(content_dir, &spine.include)?,
        };

        debug!(
            plugin = plugin.name(),
            files = scan.files.len(),
            "building virtual spine"
        );

        Ok(SpineScanResult {
            scan,
            layout,
            title: spine.title,
        })
    }

    /// Resolve the per-plugin rheo target/extension and gather marrow for it.
    ///
    /// `ext` rides on rheo-context alongside `target` (present for per-page
    /// plugin formats, omitted for the combined PDF) so typ/rheo.typ can build
    /// cross-vertebra hrefs without hardcoding the extension.
    ///
    /// Marrow only makes sense for per-page targets: `document()` and
    /// `asset()` both hard-error under the combined PDF target ("setting the
    /// document format is only supported in the bundle target"), so the same
    /// `ext` gate that marks a per-page format decides whether to gather it at
    /// all. Position (prologue, spliced before every document, vs. epilogue,
    /// spliced after) is per-contribution: a package picks its own by
    /// filename (`.marrow-prologue.typ` vs `.marrow.typ`); the project picks
    /// its own via `rheo.toml`'s `marrow_prologue` key, defaulting to
    /// epilogue so an unconfigured project compiles byte-identically. Within
    /// each position, packages contribute first in import order, then the
    /// project's own file, so it can build on what they registered.
    fn resolve_marrow(
        &self,
        plugin: &dyn FormatPlugin,
        plugin_section: &PluginSection,
        content_dir: &Path,
        resolver: &Arc<crate::packages::PackageResolver>,
    ) -> Result<MarrowContext> {
        let target = plugin.rheo_target();
        let ext = target.map(|_| plugin.extension());

        let mut marrow = Vec::new();
        let mut marrow_prologue = Vec::new();
        if ext.is_some() {
            // Behind the same opt-out that governs every other package-driven
            // behaviour.
            if plugin_section.auto_detect_packages_enabled() {
                // Through the resolver, not a directory probe: a package from a
                // repository ref lives at a sha-keyed path no probe matches, so
                // probing finds no `.marrow.typ` and the package mints none of
                // the pages it exists to mint — silently, on a green build.
                let packages = PackageIndex::resolved(
                    &crate::packages::scan_project_package_imports(&self.project.typ_files),
                    resolver,
                );
                marrow.extend(packages.marrow());
                marrow_prologue.extend(packages.marrow_prologue());
            }

            let marrow_path = content_dir.join(self.project.config.marrow_file());
            match std::fs::read_to_string(&marrow_path) {
                Ok(text) => {
                    if self.project.config.marrow_prologue() {
                        marrow_prologue.push(text);
                    } else {
                        marrow.push(text);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(RheoError::io(
                        e,
                        format!("reading marrow file '{}'", marrow_path.display()),
                    ));
                }
            }
        }

        Ok(MarrowContext {
            target,
            ext,
            marrow,
            marrow_prologue,
        })
    }

    /// Build and collision-check the `VirtualSpine` from a resolved scan,
    /// layout, title, and marrow.
    fn build_virtual_spine(
        &self,
        scan: SpineScan,
        layout: SpineLayout,
        title: Option<String>,
        marrow: Vec<String>,
        marrow_prologue: Vec<String>,
    ) -> Result<VirtualSpine> {
        let virtual_spine = VirtualSpine::build(scan, &self.project.root, layout)?
            .with_title(title)
            .with_marrow(marrow)
            .with_marrow_prologue(marrow_prologue);
        virtual_spine.check_output_collisions()?;
        Ok(virtual_spine)
    }

    /// Mould the spine into a bundle main + source overlay, and resolve the
    /// per-vertebra injections, debug source, and footnote-reset toggle that
    /// ride alongside it into [`Build::compile_bundle_once`].
    fn mould_bundle(
        &self,
        virtual_spine: &VirtualSpine,
        plugin: &dyn FormatPlugin,
    ) -> MouldedBundle {
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

        MouldedBundle {
            main: moulded.main,
            sources: moulded.sources,
            rheo_context,
            bundle_source,
            reset_footnotes,
        }
    }

    /// Vertebrae whose beacon-reported title disagrees with the
    /// Typst-resolved `DocumentMeta` title from `pass` — the gap
    /// `compile_spine`'s gated second pass exists to close.
    fn title_overrides_for_mismatch(
        virtual_spine: &VirtualSpine,
        pass: &CompiledBundlePass,
    ) -> HashMap<String, String> {
        virtual_spine
            .vertebrae
            .iter()
            .filter_map(|v| {
                let resolved = pass
                    .meta
                    .get(&v.output_path)
                    .and_then(DocumentMeta::title)?;
                let beacon = Self::beacon_title_plain_text(&pass.bundle, &v.handle);
                (beacon.as_deref() != Some(resolved))
                    .then(|| (v.handle.to_string(), resolved.to_string()))
            })
            .collect()
    }

    /// Compile the moulded bundle main once — the shared step behind both the
    /// ordinary single pass and the gated second pass of `compile_spine`.
    fn compile_bundle_once(
        &self,
        plugin: &dyn FormatPlugin,
        main: String,
        sources: HashMap<String, String>,
        rheo_context: HashMap<String, crate::reticulate::VertebraInjection>,
        global_context: crate::util::typst_literal::TypstLiteral,
        resolver: &Arc<crate::packages::PackageResolver>,
    ) -> Result<CompiledBundlePass> {
        let world = RheoWorld::new_for_bundle(
            &self.project.root,
            main,
            crate::world::WorldSpec {
                source_overlay: sources,
                rheo_context,
                global_context: Some(global_context),
                format_name: plugin.rheo_target().map(str::to_string),
                font_dirs: self.font_dirs.clone(),
                user_inputs: self.inputs.clone(),
                packages: Some(Arc::clone(resolver)),
                ..Default::default()
            },
        )?;
        let bundle = world.compile_bundle()?;
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
        let files = export_bundle(&bundle)?;
        Ok(CompiledBundlePass {
            assets,
            meta,
            files,
            bundle,
        })
    }

    /// The plain-text `title` a vertebra's own metadata beacon (`<rheo-meta:
    /// handle>`) reports, read directly off the compiled bundle's
    /// introspector rather than through a live Typst `#context` query.
    /// `None` when there's no beacon, no `title` field, or it's `none`.
    fn beacon_title_plain_text(
        bundle: &typst_bundle::Bundle,
        handle: &Handle,
    ) -> Option<ecow::EcoString> {
        let label =
            typst::foundations::Label::new(typst::utils::PicoStr::intern(&handle.meta_label()))?;
        let found = bundle
            .introspector
            .query(&typst::foundations::Selector::Label(label));
        let elem = found
            .first()?
            .to_packed::<typst::introspection::MetadataElem>()?;
        let typst::foundations::Value::Dict(dict) = &elem.value else {
            return None;
        };
        match dict.get("title").ok()? {
            typst::foundations::Value::Content(c) => Some(c.plain_text()),
            _ => None,
        }
    }

    /// Resolve `plugin`'s asset context and compile its spine — the "compile
    /// a spine for one plugin" shape shared by [`Build::run`] (via
    /// [`Build::prepare_plugin_run`]) and [`Build::compile_for_watch`], up to
    /// the point where their post-processing diverges.
    fn compile_plugin_spine<'a>(
        &'a self,
        plugin: &dyn FormatPlugin,
        packages: &PackageIndex,
        default_section: &'a PluginSection,
        content_dir: &Path,
        package_resolver: &Arc<crate::packages::PackageResolver>,
    ) -> Result<PluginCompile<'a>> {
        let ctx = self.plugin_asset_context(plugin, packages, default_section, true)?;
        let spine = self.compile_spine(plugin, ctx.section, content_dir, package_resolver)?;
        Ok(PluginCompile { ctx, spine })
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
    pub fn compile_for_watch(&self) -> Result<Option<typst_bundle::VirtualFs>> {
        if self.project.typ_files.is_empty() {
            return Err(RheoError::project_config("no .typ files found in project"));
        }

        let html_plugin = match self.plugins.iter().find(|p| p.name() == "html") {
            Some(p) => p,
            None => return Ok(None),
        };

        let default_section = PluginSection::default();
        let content_dir = resolve_effective_content_dir(&self.project);

        // Scanned per rebuild rather than once per `Build`: a `.typ` file
        // gaining an `@rheo/...` import mid-session must be picked up, and the
        // `Build` is only rebuilt when `rheo.toml` itself changes.
        let package_imports =
            crate::packages::scan_project_package_imports(&self.project.typ_files);
        let plugin_section: &PluginSection = self
            .project
            .config
            .plugin_sections
            .get(html_plugin.name())
            .unwrap_or(&default_section);
        let package_resolver = self.package_resolver();
        let packages = prewarm_and_resolve(
            &package_imports,
            plugin_section.auto_detect_packages_enabled(),
            &package_resolver,
        )?;

        // Resolving copies CSS/JS to disk too, so the dev server can serve them
        // as a fallback for requests the VirtualFs does not satisfy. A failure
        // here is propagated rather than swallowed: the CLI's watch loop turns
        // it into a warning, whereas an empty asset map would silently serve
        // every page unstyled.
        let PluginCompile {
            ctx,
            spine: compiled,
        } = self.compile_plugin_spine(
            html_plugin.as_ref(),
            &packages,
            &default_section,
            &content_dir,
            &package_resolver,
        )?;
        let asset_paths = |name| {
            ctx.resolved
                .get(name)
                .map(|v: &Vec<crate::plugins::Asset>| {
                    v.iter().map(|a| a.built_relative_path.clone()).collect()
                })
                .unwrap_or_default()
        };
        let css_paths: Vec<String> = asset_paths("css_stylesheet");
        let js_scripts: Vec<crate::util::html::ScriptRef> = ctx
            .resolved
            .get("js_scripts")
            .map(|v: &Vec<crate::plugins::Asset>| {
                v.iter()
                    .map(|a| crate::util::html::ScriptRef {
                        src: a.built_relative_path.clone(),
                        module: a.module,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let CompiledSpine {
            files: virtual_fs,
            assets,
            ..
        } = compiled;

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
                if assets.contains(&path_str) {
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

            if path_str.ends_with(".html") {
                let html = String::from_utf8_lossy(&bytes);
                let css = crate::util::html::depth_relative_refs(&css_paths, &path_str);
                let js = crate::util::html::depth_relative_scripts(&js_scripts, &path_str);
                let modified = crate::util::html::HtmlDom::apply_head_mutations(
                    &html,
                    &css,
                    &js,
                    control_head_fragment.as_deref(),
                )?;
                match modified {
                    Some(modified) => injected
                        .insert(vpath, typst::foundations::Bytes::new(modified.into_bytes())),
                    None => injected.insert(vpath, bytes),
                };
            } else {
                injected.insert(vpath, bytes);
            }
        }
        Ok(Some(injected))
    }

    /// Compile `plugin`'s spine and reduce it to disk-ready outputs: a
    /// flattened `(pages, assets)` split, `<rheo-content>` transclusion
    /// resolved, and `.rheo/*` control assets pulled out — everything a
    /// [`PluginContext`] and [`FormatPlugin::compile`] need, short of the
    /// compile call itself (kept in [`Build::run`] so only the plugin's own
    /// error is recorded-and-continued rather than propagated).
    fn prepare_plugin_run<'a>(
        &'a self,
        plugin: &dyn FormatPlugin,
        packages: &PackageIndex,
        default_section: &'a PluginSection,
        content_dir: &Path,
        package_resolver: &Arc<crate::packages::PackageResolver>,
    ) -> Result<PluginRunInputs<'a>> {
        let PluginCompile {
            ctx: plugin_assets,
            spine:
                CompiledSpine {
                    spine: virtual_spine,
                    files: virtual_fs,
                    assets,
                    bundle_source,
                    meta,
                },
        } = self.compile_plugin_spine(
            plugin,
            packages,
            default_section,
            content_dir,
            package_resolver,
        )?;

        // Read-only debug artifact — never read back as an input. Written
        // under the plugin's build-dir output, which the watcher already
        // excludes wholesale, so this cannot self-trigger a rebuild loop.
        if let Some(source) = bundle_source {
            let debug_path = plugin_assets.output_dir.join(".rheo-bundle.typ");
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

        Ok(PluginRunInputs {
            output_dir: plugin_assets.output_dir,
            spine: virtual_spine,
            section: plugin_assets.section,
            resolved: plugin_assets.resolved,
            manifest_blocks: plugin_assets.manifest_blocks,
            outputs,
            asset_files,
            control,
        })
    }

    /// Compile the project across all selected plugins.
    ///
    /// Returns the per-format [`CompilationResults`] on full success. If any
    /// format fails, the failure is logged and an error is returned (the CLI maps
    /// this to a non-zero exit).
    pub fn run(&self) -> Result<CompilationResults> {
        if self.project.typ_files.is_empty() {
            return Err(RheoError::project_config("no .typ files found in project"));
        }

        let mut results = CompilationResults::new();
        let default_section = PluginSection::default();

        // Scan .typ files for package imports once, and resolve each imported
        // package (a directory probe plus a `typst.toml` parse) once — both
        // shared across every plugin in this build.
        let package_imports =
            crate::packages::scan_project_package_imports(&self.project.typ_files);
        let package_resolver = self.package_resolver();
        let packages = prewarm_and_resolve(
            &package_imports,
            self.auto_detects_packages(),
            &package_resolver,
        )?;

        let content_dir = resolve_effective_content_dir(&self.project);

        for plugin in &self.plugins {
            let prepared = self.prepare_plugin_run(
                plugin.as_ref(),
                &packages,
                &default_section,
                &content_dir,
                &package_resolver,
            )?;
            let resolver = AssetResolver::new(&self.project.root, &prepared.output_dir);

            let ctx = PluginContext {
                project: &self.project,
                output_dir: &prepared.output_dir,
                spine: &prepared.spine,
                config: prepared.section,
                assets: &prepared.resolved,
                bundle_assets: &prepared.asset_files,
                control: &prepared.control,
            };

            // Assets are the lowest precedence tier — asset() < spine documents
            // < copy globs — so they land before the plugin writes its pages and
            // long before `copy_globs` runs below. A plugin that embeds bundle
            // assets itself (e.g. EPUB, via `ctx.bundle_assets`) takes over
            // placing them instead — a loose file next to a packaged container
            // would be unreachable from inside it.
            if !plugin.embeds_bundle_assets() {
                for (path, bytes) in &prepared.asset_files {
                    let dest = prepared.output_dir.join(path);
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            RheoError::io(e, format!("creating directory for asset {path}"))
                        })?;
                    }
                    std::fs::write(&dest, bytes.as_slice())
                        .map_err(|e| RheoError::io(e, format!("writing asset {path}")))?;
                }
            }

            match plugin.compile(ctx, &prepared.outputs) {
                Ok(_) => {
                    // Apply copy globs after bundle output is written so that
                    // explicit copy patterns win over any colliding bundle output.
                    resolver.copy_globs(
                        &self.project.config.copy,
                        &self.project.root,
                        None,
                        true,
                    )?;
                    for block in &prepared.manifest_blocks {
                        resolver.copy_globs(
                            &block.assets.copy,
                            &block.source_root,
                            block.assets.dest.as_deref(),
                            true,
                        )?;
                    }
                    for block in prepared.section.asset_blocks() {
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

/// Pre-warms `package_imports` (when `auto_detect` is on), resolves them into a
/// [`PackageIndex`], and rejects any package whose declared
/// `[tool.rheo] min_version` exceeds this build.
///
/// Pre-warming must precede the index: the index is a directory probe, so a
/// package Typst would only download at compile time is invisible to it — the
/// build then silently emits none of that package's declared assets.
fn prewarm_and_resolve(
    package_imports: &[String],
    auto_detect: bool,
    resolver: &crate::packages::PackageResolver,
) -> Result<PackageIndex> {
    if auto_detect {
        crate::packages::prewarm_packages(package_imports, resolver);
    }
    let packages = PackageIndex::resolved(package_imports, resolver);
    packages.check_min_versions()?;
    packages.check_source_availability()?;
    Ok(packages)
}

/// Auto-detected manifest asset blocks for `plugin_section`'s format, or
/// none when the section disables package auto-detection.
fn manifest_blocks_for(
    packages: &PackageIndex,
    plugin_section: &PluginSection,
    format_name: &str,
) -> Vec<crate::plugins::PackageAssets> {
    if plugin_section.auto_detect_packages_enabled() {
        packages.manifest_assets(format_name)
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
fn resolve_font_dirs(project: &ProjectConfig, cli_font_dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
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

    let cwd = std::env::current_dir().map_err(|e| RheoError::io(e, "getting current directory"))?;
    for dir in cli_font_dirs {
        dirs.push(resolve_path(&cwd, dir));
    }

    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// What `Build::run` handed the plugin for each output: its path, its
    /// resolved title, and its compiled bytes as text. `CastVertebra` itself is
    /// not `Clone`, and these three are all any test here asserts on.
    #[derive(Default)]
    struct Captured(Mutex<Vec<(String, String, String)>>);

    impl Captured {
        fn field(
            &self,
            output_path: &str,
            pick: impl Fn(&(String, String, String)) -> String,
        ) -> String {
            let outputs = self.0.lock().unwrap();
            outputs
                .iter()
                .find(|(path, _, _)| path == output_path)
                .map(pick)
                .unwrap_or_else(|| {
                    panic!(
                        "no output for {output_path}; got {:?}",
                        outputs.iter().map(|(p, _, _)| p).collect::<Vec<_>>()
                    )
                })
        }

        fn title(&self, output_path: &str) -> String {
            self.field(output_path, |(_, title, _)| title.clone())
        }

        fn html(&self, output_path: &str) -> String {
            self.field(output_path, |(_, _, html)| html.clone())
        }
    }

    /// An `html` plugin that records every output it is handed.
    struct CapturingPlugin(Arc<Captured>);

    impl FormatPlugin for CapturingPlugin {
        fn name(&self) -> &'static str {
            "html"
        }
        fn compile(&self, _ctx: PluginContext<'_>, outputs: &[CastVertebra]) -> Result<()> {
            self.0.0.lock().unwrap().extend(outputs.iter().map(|o| {
                (
                    o.output_path.clone(),
                    o.title.clone(),
                    String::from_utf8_lossy(o.bytes.as_slice()).into_owned(),
                )
            }));
            Ok(())
        }
    }

    /// Run `project` through a `CapturingPlugin` and return what it saw.
    fn run_capturing(project: ProjectConfig) -> Arc<Captured> {
        let captured = Arc::new(Captured::default());
        let plugin: Box<dyn FormatPlugin> = Box::new(CapturingPlugin(captured.clone()));
        Build::prepare(project, vec![plugin], BuildOptions::default())
            .expect("prepare build")
            .run()
            .expect("run build");
        captured
    }

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
            marrow_prologue: Vec::new(),
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
            marrow_prologue: Vec::new(),
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

        assert_eq!(
            run_capturing(project).title("templated.html"),
            "Templated Title From Book",
            "CastVertebra.title should be Typst's resolved DocumentInfo.title, \
             not the AST scan's filename-derived fallback"
        );
    }

    /// The dev-server in-memory path must hoist `<rheo-head>` exactly like the
    /// on-disk path, even with no CSS/JS asset and no `.rheo/head.html`
    /// fragment — the case the old per-format gate skipped entirely.
    #[test]
    fn test_compile_for_watch_hoists_rheo_head_without_assets() {
        use crate::config::project::ProjectConfig;

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
            config: crate::RheoConfig {
                content_dir: Some("content".to_string()),
                formats: vec!["html".to_string()],
                ..Default::default()
            },
            typ_files: vec![root.join("content/a.typ")],
            mode: ProjectMode::Directory,
            config_path: None,
        };

        struct HtmlNoAssets;
        impl FormatPlugin for HtmlNoAssets {
            fn name(&self) -> &'static str {
                "html"
            }
            fn compile(&self, _ctx: PluginContext<'_>, _outputs: &[CastVertebra]) -> Result<()> {
                Ok(())
            }
        }

        let plugin: Box<dyn FormatPlugin> = Box::new(HtmlNoAssets);
        let build =
            Build::prepare(project, vec![plugin], BuildOptions::default()).expect("prepare build");
        let vfs = build
            .compile_for_watch()
            .expect("compile_for_watch")
            .expect("html plugin selected");

        let (_, bytes) = vfs
            .iter()
            .find(|(p, _)| p.get_with_slash().ends_with("a.html"))
            .expect("a.html present");
        let html = String::from_utf8_lossy(bytes.as_slice());
        assert!(!html.contains("rheo-head"), "wrapper not removed:\n{html}");
        let head_end = html.find("</head>").expect("has head");
        let meta_pos = html.find("name=\"x\"").expect("meta present");
        assert!(meta_pos < head_end, "meta not hoisted into head:\n{html}");
    }

    /// A `path`-backed package with no `[tool.rheo.*]` assets at all still gets
    /// its directory watched — unlike a repository ref or a release, its tree
    /// can change while the watch is running, so watch coverage cannot be
    /// gated on whether the package happens to declare an asset block.
    #[test]
    fn test_watch_asset_spec_watches_path_backed_package_without_assets() {
        use crate::config::project::ProjectConfig;
        use crate::config::{NamespaceSource, PathSource};

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("content")).expect("create content dir");
        std::fs::write(
            root.join("content/index.typ"),
            "#import \"@demo/thing:0.1.0\": greet\n#greet()\n",
        )
        .expect("write content/index.typ");

        let pkg_dir = root.join("pkgs/thing/0.1.0");
        std::fs::create_dir_all(pkg_dir.join("src")).expect("create package dir");
        std::fs::write(
            pkg_dir.join("typst.toml"),
            "[package]\nname = \"thing\"\nversion = \"0.1.0\"\nentrypoint = \"src/lib.typ\"\n",
        )
        .expect("write typst.toml");
        std::fs::write(pkg_dir.join("src/lib.typ"), "#let greet() = [Hi]\n")
            .expect("write lib.typ");

        let project = ProjectConfig {
            name: "test".to_string(),
            root: root.to_path_buf(),
            config: crate::RheoConfig {
                content_dir: Some("content".to_string()),
                formats: vec!["html".to_string()],
                packages: HashMap::from([(
                    "demo".to_string(),
                    NamespaceSource::Path(PathSource {
                        root: root.join("pkgs"),
                        subdir: String::new(),
                    }),
                )]),
                ..Default::default()
            },
            typ_files: vec![root.join("content/index.typ")],
            mode: ProjectMode::Directory,
            config_path: None,
        };

        struct HtmlNoAssets;
        impl FormatPlugin for HtmlNoAssets {
            fn name(&self) -> &'static str {
                "html"
            }
            fn compile(&self, _ctx: PluginContext<'_>, _outputs: &[CastVertebra]) -> Result<()> {
                Ok(())
            }
        }

        let plugin: Box<dyn FormatPlugin> = Box::new(HtmlNoAssets);
        let build =
            Build::prepare(project, vec![plugin], BuildOptions::default()).expect("prepare build");

        let spec = build.watch_asset_spec();
        let expected = pkg_dir.canonicalize().unwrap_or(pkg_dir);
        assert!(
            spec.package_roots().contains(&expected),
            "expected {expected:?} in package_roots, got {:?}",
            spec.package_roots()
        );
    }

    /// Same as above, but with a CSS asset present, so the shared step's
    /// injection and hoist both run in the same pass.
    #[test]
    fn test_compile_for_watch_hoists_rheo_head_with_css() {
        use crate::config::project::ProjectConfig;
        use crate::plugins::{AssetConfig, EmbeddedDefault};

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
            config: crate::RheoConfig {
                content_dir: Some("content".to_string()),
                formats: vec!["html".to_string()],
                ..Default::default()
            },
            typ_files: vec![root.join("content/a.typ")],
            mode: ProjectMode::Directory,
            config_path: None,
        };

        struct HtmlWithCss;
        impl FormatPlugin for HtmlWithCss {
            fn name(&self) -> &'static str {
                "html"
            }
            fn assets(&self) -> Vec<AssetConfig> {
                vec![AssetConfig {
                    name: "css_stylesheet",
                    default_path: "style.css",
                    required: false,
                    default_content: Some(EmbeddedDefault {
                        name: "test-default.css",
                        content: "body{color:red}",
                    }),
                }]
            }
            fn compile(&self, _ctx: PluginContext<'_>, _outputs: &[CastVertebra]) -> Result<()> {
                Ok(())
            }
        }

        let plugin: Box<dyn FormatPlugin> = Box::new(HtmlWithCss);
        let build =
            Build::prepare(project, vec![plugin], BuildOptions::default()).expect("prepare build");
        let vfs = build
            .compile_for_watch()
            .expect("compile_for_watch")
            .expect("html plugin selected");

        let (_, bytes) = vfs
            .iter()
            .find(|(p, _)| p.get_with_slash().ends_with("a.html"))
            .expect("a.html present");
        let html = String::from_utf8_lossy(bytes.as_slice());
        assert!(!html.contains("rheo-head"), "wrapper not removed:\n{html}");
        assert!(
            html.contains("test-default.css"),
            "css link missing:\n{html}"
        );
        let head_end = html.find("</head>").expect("has head");
        let meta_pos = html.find("name=\"x\"").expect("meta present");
        assert!(meta_pos < head_end, "meta not hoisted into head:\n{html}");
    }

    /// A one-vertebra project with a `*bold text*` paragraph and a marrow
    /// `#show strong` rule, shared by the epilogue/prologue end-to-end tests
    /// below. Only `marrow_prologue` differs between them.
    fn build_show_rule_project(root: &Path, marrow_prologue: bool) -> ProjectConfig {
        std::fs::create_dir_all(root.join("content")).expect("create content dir");
        std::fs::write(
            root.join("content/index.typ"),
            "= Index\n\nThis has *bold text*.\n",
        )
        .expect("write content/index.typ");
        std::fs::write(
            root.join("content/.marrow.typ"),
            "#show strong: it => [TOUCHED]\n",
        )
        .expect("write marrow");

        ProjectConfig {
            name: "test".to_string(),
            root: root.to_path_buf(),
            config: crate::RheoConfig {
                content_dir: Some("content".to_string()),
                formats: vec!["html".to_string()],
                marrow_prologue: Some(marrow_prologue),
                ..Default::default()
            },
            typ_files: vec![root.join("content/index.typ")],
            mode: ProjectMode::Directory,
            config_path: None,
        }
    }

    /// End-to-end: with the default (epilogue) position, a `#show` rule in
    /// marrow is scoped to marrow's own output only — it must not reach a
    /// vertebra that already exists (marrow is spliced after every document).
    /// This is the byte-identical-by-default guarantee: an unconfigured
    /// project's `*bold text*` still renders as `<strong>`.
    #[test]
    fn test_run_default_marrow_epilogue_does_not_reach_pre_existing_vertebra() {
        let dir = tempfile::tempdir().expect("tempdir");
        let project = build_show_rule_project(dir.path(), false);

        let html = run_capturing(project).html("index.html");
        assert!(
            html.contains("bold text"),
            "epilogue marrow must not reach a pre-existing vertebra, got:\n{html}"
        );
    }

    /// End-to-end: with `marrow_prologue = true`, the same `#show` rule is
    /// spliced BEFORE every document, so it reaches the pre-existing vertebra.
    #[test]
    fn test_run_marrow_prologue_reaches_pre_existing_vertebra() {
        let dir = tempfile::tempdir().expect("tempdir");
        let project = build_show_rule_project(dir.path(), true);

        let html = run_capturing(project).html("index.html");
        assert!(
            html.contains("TOUCHED"),
            "marrow_prologue = true should let #show reach the pre-existing vertebra, got:\n{html}"
        );
        assert!(!html.contains("bold text"));
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

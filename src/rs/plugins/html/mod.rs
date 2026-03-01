use crate::compile::RheoCompileOptions;
use crate::config::{HtmlOptions, RheoConfig, SpineConfig};
use crate::formats::common::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::postprocess;
use crate::project::ProjectConfig;
use crate::world::RheoWorld;
use crate::{OutputFormat, Result, RheoError};
use std::path::Path;
use tracing::{debug, info};
use typst_html::HtmlDocument;

use super::{CompilationDispatch, FormatPlugin, PluginContext};

pub struct HtmlPlugin;

impl FormatPlugin for HtmlPlugin {
    fn name(&self) -> &'static str {
        "html"
    }

    fn output_format(&self) -> OutputFormat {
        OutputFormat::Html
    }

    fn extension(&self) -> &'static str {
        "html"
    }

    fn supports_live_preview(&self) -> bool {
        true
    }

    fn compilation_dispatch(&self, _config: &RheoConfig) -> CompilationDispatch {
        CompilationDispatch::PerFile
    }

    fn spine_config<'a>(&self, config: &'a RheoConfig) -> Option<&'a dyn SpineConfig> {
        config.html.spine.as_ref().map(|s| s as &dyn SpineConfig)
    }

    fn copy_assets(&self, project: &ProjectConfig, output_dir: &Path) -> Result<()> {
        copy_html_assets(project.style_css.as_deref(), output_dir)
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        let html_options = HtmlOptions {
            stylesheets: ctx.project.config.html.stylesheets.clone(),
            fonts: ctx.project.config.html.fonts.clone(),
        };
        compile_html_new(ctx.options, html_options)
    }
}

/// Copy style.css to the HTML output directory.
///
/// Priority:
/// 1. If project has style.css in its root, use that (project-specific override)
/// 2. Otherwise, use bundled default style.css
pub fn copy_html_assets(project_style_css: Option<&Path>, dest_dir: &Path) -> Result<()> {
    const DEFAULT_CSS: &str = include_str!("../../../templates/init/style.css");
    let dest_path = dest_dir.join("style.css");

    if let Some(project_css) = project_style_css {
        std::fs::copy(project_css, &dest_path).map_err(|e| {
            RheoError::io(
                e,
                format!(
                    "copying project style.css from {:?} to {:?}",
                    project_css, dest_path
                ),
            )
        })?;
        debug!(source = %project_css.display(), dest = %dest_path.display(), "copied project-specific style.css");
    } else {
        std::fs::write(&dest_path, DEFAULT_CSS).map_err(|e| {
            RheoError::io(e, format!("writing default style.css to {:?}", dest_path))
        })?;
        debug!(dest = %dest_path.display(), "copied default style.css");
    }

    Ok(())
}

pub fn compile_html_to_document(
    input: &Path,
    root: &Path,
    output_format: OutputFormat,
) -> Result<HtmlDocument> {
    // Create the compilation world with specified format for link transformations
    let world = RheoWorld::new(root, input, Some(output_format))?;

    // Compile the document to HtmlDocument
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(&world);

    // Filter out HTML development warning
    let html_filter = |w: &typst::diag::SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    unwrap_compilation_result(Some(&world), result, Some(html_filter))
}

pub fn compile_document_to_string(document: &HtmlDocument) -> Result<String> {
    // Export to HTML string (no post-processing - that happens in the compilation pipeline)
    typst_html::html(document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))
}

// ============================================================================
// Single-file HTML compilation (implementation functions)
// ============================================================================

/// Implementation: Compile a Typst document to HTML (fresh compilation)
///
/// Uses format-aware RheoWorld for automatic link transformation (.typ → .html).
/// Transformations happen on-demand during Typst compilation (including imports).
///
/// Pipeline: Compile (with transformations) → Export → Inject Head → Write
fn compile_html_impl_fresh(
    input: &Path,
    output: &Path,
    root: &Path,
    html_options: &HtmlOptions,
) -> Result<()> {
    // Compile to HTML document (transformations happen in RheoWorld)
    let doc = compile_html_to_document(input, root, OutputFormat::Html)?;
    let html_string = compile_document_to_string(&doc)?;

    // Inject CSS and font links into <head>
    let stylesheets: Vec<&str> = html_options
        .stylesheets
        .iter()
        .map(|s| s.as_str())
        .collect();
    let fonts: Vec<&str> = html_options.fonts.iter().map(|s| s.as_str()).collect();
    let html_string = postprocess::inject_head_links(&html_string, &stylesheets, &fonts)?;

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}

/// Implementation: Compile a Typst document to HTML (incremental compilation)
///
/// Uses format-aware RheoWorld for automatic link transformation (.typ → .html).
/// Reuses existing RheoWorld instance for compilation (enabling incremental compilation
/// through Typst's comemo caching system).
///
/// Pipeline: Update World → Compile (with transformations) → Export → Inject Head → Write
fn compile_html_impl(
    world: &RheoWorld,
    input: &Path,
    output: &Path,
    html_options: &HtmlOptions,
) -> Result<()> {
    // Compile to HTML document (transformations happen in RheoWorld)
    info!(input = %input.display(), "compiling to HTML");
    let result = typst::compile::<HtmlDocument>(world);

    // Filter out HTML development warning
    let html_filter = |w: &typst::diag::SourceDiagnostic| {
        !w.message
            .contains("html export is under active development and incomplete")
    };

    let document = unwrap_compilation_result(Some(world), result, Some(html_filter))?;

    // Export to HTML string
    debug!(output = %output.display(), "exporting to HTML");
    let html_string =
        typst_html::html(&document).map_err(|e| handle_export_errors(e, ExportErrorType::Html))?;

    // Inject CSS and font links into <head>
    let stylesheets: Vec<&str> = html_options
        .stylesheets
        .iter()
        .map(|s| s.as_str())
        .collect();
    let fonts: Vec<&str> = html_options.fonts.iter().map(|s| s.as_str()).collect();
    let html_string = postprocess::inject_head_links(&html_string, &stylesheets, &fonts)?;

    // Write to file
    debug!(size = html_string.len(), "writing HTML file");
    std::fs::write(output, &html_string)
        .map_err(|e| RheoError::io(e, format!("writing HTML file to {:?}", output)))?;

    info!(output = %output.display(), "successfully compiled to HTML");
    Ok(())
}

// ============================================================================
// Unified public API
// ============================================================================

/// Compile Typst document to HTML.
///
/// Routes to the appropriate implementation based on options:
/// - Fresh compilation: compile_html_impl_fresh() (when options.world is None)
/// - Incremental compilation: compile_html_impl() (when options.world is Some)
///
/// # Arguments
/// * `options` - Compilation options (input, output, root, repo_root, world)
/// * `html_options` - HTML-specific options (stylesheets, fonts for head injection)
///
/// # Returns
/// * `Result<()>` - Success or compilation error
pub fn compile_html_new(options: RheoCompileOptions, html_options: HtmlOptions) -> Result<()> {
    match options.world {
        // Incremental compilation (reuse existing world)
        Some(world) => compile_html_impl(world, &options.input, &options.output, &html_options),
        // Fresh compilation (create new world)
        None => compile_html_impl_fresh(
            &options.input,
            &options.output,
            &options.root,
            &html_options,
        ),
    }
}

mod dom;
mod html_head;

use crate::compile::RheoCompileOptions;
use crate::config::HtmlOptions;
use crate::diagnostics::{ExportErrorType, handle_export_errors, unwrap_compilation_result};
use crate::world::RheoWorld;
use crate::{Result, RheoError};
use std::path::Path;
use tracing::{debug, info};
use typst_html::HtmlDocument;

use super::{FormatPlugin, PluginContext, PluginInput};

pub struct HtmlPlugin;

impl FormatPlugin for HtmlPlugin {
    fn name(&self) -> &'static str {
        "html"
    }

    fn supports_live_preview(&self) -> bool {
        true
    }

    fn inputs(&self) -> &'static [PluginInput] {
        &[PluginInput {
            name: "stylesheet",
            path: "style.css",
            required: false,
        }]
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        if ctx.plugin_config.spine.merge {
            return Err(RheoError::project_config(
                "HTML does not support merged compilation",
            ));
        }
        // If the project didn't provide style.css, write the bundled default.
        if !ctx.inputs.contains_key("stylesheet") {
            const DEFAULT_CSS: &str = include_str!("../../../templates/init/style.css");
            let dest = ctx.output_config.dir_for_plugin("html").join("style.css");
            std::fs::write(&dest, DEFAULT_CSS)
                .map_err(|e| RheoError::io(e, "writing default style.css"))?;
            debug!(dest = %dest.display(), "wrote bundled default style.css");
        }
        let html_options = HtmlOptions {
            stylesheets: ctx.project.config.html.stylesheets.clone(),
            fonts: ctx.project.config.html.fonts.clone(),
        };
        compile_html_new(ctx.options, html_options)
    }
}

pub fn compile_html_to_document(
    input: &Path,
    root: &Path,
    format_name: &str,
) -> Result<HtmlDocument> {
    // Create the compilation world with specified format for link transformations
    let world = RheoWorld::new(root, input, Some(format_name))?;

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
    let doc = compile_html_to_document(input, root, "html")?;
    let html_string = compile_document_to_string(&doc)?;

    // Inject CSS and font links into <head>
    let stylesheets: Vec<&str> = html_options
        .stylesheets
        .iter()
        .map(|s| s.as_str())
        .collect();
    let fonts: Vec<&str> = html_options.fonts.iter().map(|s| s.as_str()).collect();
    let html_string = html_head::inject_head_links(&html_string, &stylesheets, &fonts)?;

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
    let html_string = html_head::inject_head_links(&html_string, &stylesheets, &fonts)?;

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

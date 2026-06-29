use rheo_core::{
    FormatInitTemplate, FormatPlugin, PluginContext, Result, RheoError, SpineLayoutKind,
    CastVertebra, TypstFormat,
};

pub struct PdfPlugin;
const PLUGIN_NAME: &str = "pdf";

impl FormatPlugin for PdfPlugin {
    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    fn spine_layout_kind(&self) -> SpineLayoutKind {
        SpineLayoutKind::SingleCombined
    }

    fn typst_format(&self) -> TypstFormat {
        TypstFormat::Pdf
    }

    fn format_init_template(&self) -> FormatInitTemplate {
        FormatInitTemplate {
            files: vec![],
            options_toml: Some(include_str!("templates/init/rheo_section.toml")),
        }
    }

    fn typst_library(&self) -> Option<&'static str> {
        Some(include_str!("lib.typ"))
    }

    fn compile(&self, ctx: PluginContext<'_>, outputs: &[CastVertebra]) -> Result<()> {
        let output = outputs
            .first()
            .ok_or_else(|| RheoError::project_config("PDF compilation produced no output"))?;
        let out_path = ctx.output_dir.join(&output.output_path);
        std::fs::write(&out_path, output.bytes.as_slice())
            .map_err(|e| RheoError::io(e, format!("writing PDF to {:?}", out_path)))?;
        tracing::info!(output = %out_path.display(), "successfully compiled to PDF");
        Ok(())
    }
}

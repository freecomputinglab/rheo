use rheo_core::{FormatPlugin, PluginContext, Result};

pub struct PdfPlugin;
const PLUGIN_NAME: &str = "pdf";

impl FormatPlugin for PdfPlugin {
    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    fn typst_library(&self) -> Option<&'static str> {
        Some(include_str!("lib.typ"))
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        ctx.compile(self)
    }
}

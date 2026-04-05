use rheo_core::{FormatPlugin, PluginContext, Result};

pub struct PdfPlugin;
const PLUGIN_NAME: &str = "pdf";

impl FormatPlugin for PdfPlugin {
    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    fn init_rheo_toml_section_template(&self) -> Option<&'static str> {
        Some(include_str!("templates/init/rheo_section.toml"))
    }

    fn typst_library(&self) -> Option<&'static str> {
        Some(include_str!("lib.typ"))
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        ctx.compile(self)
    }
}

use rheo_core::{FormatInitTemplate, FormatPlugin, LinkStrategy, PluginContext, Result};

pub struct PdfPlugin;
const PLUGIN_NAME: &str = "pdf";

impl FormatPlugin for PdfPlugin {
    fn name(&self) -> &'static str {
        PLUGIN_NAME
    }

    /// PDF is a paged format: merged compiles convert in-spine links to labels,
    /// single-file compiles strip them.
    fn link_strategy(&self) -> LinkStrategy {
        LinkStrategy::PagedLabels
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

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        ctx.compile(self)
    }
}

use rheo_core::{FormatPlugin, PluginContext, Result};

pub struct PdfPlugin;
const PLUGIN_NAME: &str = "pdf";

impl FormatPlugin for PdfPlugin {
    fn name(&self) -> &'static str {
        &PLUGIN_NAME
    }

    fn typst_library(&self) -> Option<&'static str> {
        // PDF-specific lemma function for numbered lemmas in academic documents
        Some(
            r#"
#let lemmacount = counter("lemmas")
#let lemma(it) = block(inset: 8pt, [
  #lemmacount.step()
  #strong[Lemma #context lemmacount.display()]: #it
])
"#,
        )
    }

    fn compile(&self, ctx: PluginContext<'_>) -> Result<()> {
        ctx.compile(self)
    }
}

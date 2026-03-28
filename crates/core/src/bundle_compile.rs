use crate::diagnostics::print_diagnostics;
use crate::RheoError;
use crate::RheoWorld;
use crate::Result;
use typst::diag::Warned;

/// Compile and export a Typst bundle to file bytes.
///
/// This helper function consolidates the common bundle compilation and export
/// logic used by both HTML and PDF plugins. It compiles the bundle using the
/// world, prints diagnostics, and returns the exported file bytes.
///
/// # Arguments
/// * `world` - The RheoWorld for compilation context
///
/// # Returns
/// A vector of (filename, bytes) pairs representing the exported bundle files
///
/// # Errors
/// Returns `RheoError::project_config` if compilation or export fails
pub fn export_typst_bundle(world: &RheoWorld) -> Result<Vec<(String, Vec<u8>)>> {
    let Warned { output, warnings } = typst::compile::<typst_bundle::Bundle>(world);
    let _ = print_diagnostics(world, &[], &warnings);
    let bundle = output.map_err(|errors| {
        let _ = print_diagnostics(world, &errors[..], &[]);
        let msgs: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
        RheoError::project_config(format!("bundle compilation had errors: {}", msgs.join(", ")))
    })?;
    let bundle_options = typst_bundle::BundleOptions {
        pixel_per_pt: 144.0,
        pdf: typst_pdf::PdfOptions::default(),
    };
    let fs = typst_bundle::export(&bundle, &bundle_options)
        .map_err(|e| RheoError::project_config(format!("bundle export failed: {:?}", e)))?;
    Ok(fs.into_iter().map(|(p, b)| (p.get_without_slash().to_string(), b.to_vec())).collect())
}

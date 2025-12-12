use crate::error::RheoError;
use ecow::EcoVec;
use tracing::{error, warn};
use typst::diag::{SourceDiagnostic, SourceResult, Warned};

/// Process Typst compilation warnings with optional filtering.
///
/// # Arguments
/// * `warnings` - Warnings from compilation result
/// * `filter_fn` - Optional filter predicate (returns true to KEEP warning)
///
/// # Example
/// ```
/// // Filter out HTML development warning
/// handle_typst_warnings(&result.warnings, Some(|w| {
///     !w.message.contains("html export is under active development")
/// }));
/// ```
pub fn handle_typst_warnings<F>(warnings: &[SourceDiagnostic], filter_fn: Option<F>)
where
    F: Fn(&SourceDiagnostic) -> bool,
{
    for warning in warnings {
        if let Some(ref filter) = filter_fn {
            if !filter(warning) {
                continue;
            }
        }
        warn!(message = %warning.message, "compilation warning");
    }
}

/// Convert Typst compilation errors to RheoError::Compilation.
///
/// Logs all errors and aggregates them into a single error with count.
pub fn handle_typst_errors(errors: EcoVec<SourceDiagnostic>) -> RheoError {
    for err in &errors {
        error!(message = %err.message, "compilation error");
    }
    let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
    RheoError::Compilation {
        count: errors.len(),
        errors: error_messages.join("\n"),
    }
}

/// Process compilation result: handle warnings and unwrap output.
///
/// # Arguments
/// * `result` - Typst compilation result
/// * `filter_warnings` - Optional warning filter function (returns true to KEEP)
///
/// # Returns
/// The compiled output or RheoError::Compilation
///
/// # Example
/// ```
/// let result = typst::compile::<PagedDocument>(&world);
/// let document = unwrap_compilation_result(result, None)?;
/// ```
pub fn unwrap_compilation_result<T, F>(
    result: Warned<SourceResult<T>>,
    filter_warnings: Option<F>,
) -> crate::Result<T>
where
    F: Fn(&SourceDiagnostic) -> bool,
{
    handle_typst_warnings(&result.warnings, filter_warnings);
    result.output.map_err(handle_typst_errors)
}

/// Export error type for generic error handling
#[derive(Debug, Clone, Copy)]
pub enum ExportErrorType {
    Pdf,
    Html,
}

impl ExportErrorType {
    fn name(&self) -> &'static str {
        match self {
            ExportErrorType::Pdf => "PDF",
            ExportErrorType::Html => "HTML",
        }
    }
}

/// Convert Typst export errors to appropriate RheoError variant.
///
/// # Arguments
/// * `errors` - Export errors
/// * `error_type` - Which RheoError variant to use
///
/// # Example
/// ```
/// let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())
///     .map_err(|e| handle_export_errors(e, ExportErrorType::Pdf))?;
/// ```
pub fn handle_export_errors(
    errors: EcoVec<SourceDiagnostic>,
    error_type: ExportErrorType,
) -> RheoError {
    let type_name = error_type.name();
    for err in &errors {
        error!(message = %err.message, "{} export error", type_name);
    }
    let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
    match error_type {
        ExportErrorType::Pdf => RheoError::PdfGeneration {
            count: errors.len(),
            errors: error_messages.join("\n"),
        },
        ExportErrorType::Html => RheoError::HtmlGeneration {
            count: errors.len(),
            errors: error_messages.join("\n"),
        },
    }
}

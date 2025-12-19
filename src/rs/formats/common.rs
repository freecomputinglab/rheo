use crate::error::RheoError;
use crate::world::RheoWorld;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::term;
use ecow::{EcoVec, eco_format};
use tracing::{error, warn};
use typst::WorldExt;
use typst::diag::{Severity, SourceDiagnostic, SourceResult, Warned};

/// Print diagnostic messages to stderr using codespan-reporting.
///
/// This renders diagnostics with rich source context, color coding, and
/// helpful hints/traces. Similar to how the Typst CLI displays errors.
///
/// # Arguments
/// * `world` - The RheoWorld for source file access
/// * `errors` - Error diagnostics to display
/// * `warnings` - Warning diagnostics to display
///
/// # Returns
/// Returns an error if diagnostic rendering fails (rare)
pub fn print_diagnostics(
    world: &RheoWorld,
    errors: &[SourceDiagnostic],
    warnings: &[SourceDiagnostic],
) -> std::result::Result<(), codespan_reporting::files::Error> {
    let config = term::Config {
        tab_width: 2,
        ..Default::default()
    };

    // Use stderr for diagnostic output
    let mut stderr = term::termcolor::StandardStream::stderr(term::termcolor::ColorChoice::Auto);

    for diagnostic in warnings.iter().chain(errors.iter()) {
        let diag = match diagnostic.severity {
            Severity::Error => Diagnostic::error(),
            Severity::Warning => Diagnostic::warning(),
        }
        .with_message(diagnostic.message.clone())
        .with_notes(
            diagnostic
                .hints
                .iter()
                .map(|s| (eco_format!("hint: {}", s)).into())
                .collect(),
        )
        .with_labels(label(world, diagnostic.span).into_iter().collect());

        term::emit(&mut stderr, &config, world, &diag)?;

        // Stacktrace-like helper diagnostics (trace)
        for point in &diagnostic.trace {
            let message = point.v.to_string();
            let help = Diagnostic::help()
                .with_message(message)
                .with_labels(label(world, point.span).into_iter().collect());

            term::emit(&mut stderr, &config, world, &help)?;
        }
    }

    Ok(())
}

/// Create a label for a span.
///
/// This converts a Typst span into a codespan-reporting label pointing
/// to the primary location of an error or warning.
fn label(world: &RheoWorld, span: typst::syntax::Span) -> Option<Label<typst::syntax::FileId>> {
    Some(Label::primary(span.id()?, world.range(span)?))
}

/// Process Typst compilation warnings.
///
/// When a world is provided, uses codespan-reporting for rich terminal output.
/// Otherwise falls back to simple logging.
///
/// # Arguments
/// * `world` - Optional RheoWorld for rich diagnostic rendering
/// * `warnings` - Warnings from compilation result
///
/// # Example
/// ```ignore
/// handle_typst_warnings(Some(&world), &result.warnings);
/// ```
pub fn handle_typst_warnings(world: Option<&RheoWorld>, warnings: &[SourceDiagnostic]) {
    if warnings.is_empty() {
        return;
    }

    // Use rich diagnostics if world is available
    if let Some(world) = world {
        // Ignore errors from diagnostic printing (shouldn't happen)
        let _ = print_diagnostics(world, &[], warnings);
    } else {
        // Fall back to simple logging
        for warning in warnings {
            warn!(message = %warning.message, "compilation warning");
        }
    }
}

/// Convert Typst compilation errors to RheoError::Compilation.
///
/// When a world is provided, uses codespan-reporting for rich terminal output.
/// Otherwise falls back to simple logging. Always returns a RheoError for
/// error handling.
///
/// # Arguments
/// * `world` - Optional RheoWorld for rich diagnostic rendering
/// * `errors` - Compilation errors
pub fn handle_typst_errors(
    world: Option<&RheoWorld>,
    errors: EcoVec<SourceDiagnostic>,
) -> RheoError {
    // Use rich diagnostics if world is available
    if let Some(world) = world {
        // Ignore errors from diagnostic printing (shouldn't happen)
        let _ = print_diagnostics(world, &errors, &[]);
    } else {
        // Fall back to simple logging
        for err in &errors {
            error!(message = %err.message, "compilation error");
        }
    }

    // Always return RheoError for error handling
    let error_messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
    RheoError::Compilation {
        count: errors.len(),
        errors: error_messages.join("\n"),
    }
}

/// Process compilation result: handle warnings and unwrap output.
///
/// When a world is provided, uses codespan-reporting for rich error/warning display.
///
/// # Arguments
/// * `world` - Optional RheoWorld for rich diagnostic rendering
/// * `result` - Typst compilation result
/// * `filter_warnings` - Optional warning filter function (returns true to KEEP)
///
/// # Returns
/// The compiled output or RheoError::Compilation
///
/// # Example
/// ```ignore
/// let result = typst::compile::<PagedDocument>(&world);
/// let document = unwrap_compilation_result(Some(&world), result, None)?;
/// ```
pub fn unwrap_compilation_result<T, F>(
    world: Option<&RheoWorld>,
    result: Warned<SourceResult<T>>,
    filter_warnings: Option<F>,
) -> crate::Result<T>
where
    F: Fn(&SourceDiagnostic) -> bool,
{
    // Filter warnings if filter function provided
    if let Some(filter_fn) = filter_warnings {
        let filtered: Vec<_> = result
            .warnings
            .iter()
            .filter(|w| filter_fn(w))
            .cloned()
            .collect();
        handle_typst_warnings(world, &filtered);
    } else {
        handle_typst_warnings(world, &result.warnings);
    }
    result.output.map_err(|e| handle_typst_errors(world, e))
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
/// ```ignore
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

#[cfg(test)]
mod tests {
    use super::*;
    use ecow::eco_vec;
    use typst::diag::{EcoString, Severity};
    use typst::syntax::Span;

    // Helper to create mock SourceDiagnostic for testing
    fn create_diagnostic(message: &str, severity: Severity) -> SourceDiagnostic {
        SourceDiagnostic {
            span: Span::detached(),
            message: EcoString::from(message),
            severity,
            hints: eco_vec![],
            trace: eco_vec![],
        }
    }

    fn create_error(message: &str) -> SourceDiagnostic {
        create_diagnostic(message, Severity::Error)
    }

    fn create_warning(message: &str) -> SourceDiagnostic {
        create_diagnostic(message, Severity::Warning)
    }

    #[test]
    fn test_handle_typst_errors_single() {
        let errors = eco_vec![create_error("test error")];
        let result = handle_typst_errors(None, errors);

        match result {
            RheoError::Compilation { count, errors } => {
                assert_eq!(count, 1);
                assert_eq!(errors, "test error");
            }
            _ => panic!("Expected Compilation error"),
        }
    }

    #[test]
    fn test_handle_typst_errors_multiple() {
        let errors = eco_vec![
            create_error("error 1"),
            create_error("error 2"),
            create_error("error 3"),
        ];
        let result = handle_typst_errors(None, errors);

        match result {
            RheoError::Compilation { count, errors } => {
                assert_eq!(count, 3);
                assert_eq!(errors, "error 1\nerror 2\nerror 3");
            }
            _ => panic!("Expected Compilation error"),
        }
    }

    #[test]
    fn test_unwrap_compilation_result_success() {
        let result = Warned {
            output: Ok(42),
            warnings: eco_vec![create_warning("test warning")],
        };

        let output = unwrap_compilation_result(None, result, None::<fn(&SourceDiagnostic) -> bool>);
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), 42);
    }

    #[test]
    fn test_unwrap_compilation_result_error() {
        let result: Warned<SourceResult<i32>> = Warned {
            output: Err(eco_vec![create_error("compilation failed")]),
            warnings: eco_vec![],
        };

        let output = unwrap_compilation_result(None, result, None::<fn(&SourceDiagnostic) -> bool>);
        assert!(output.is_err());

        match output.unwrap_err() {
            RheoError::Compilation { count, errors } => {
                assert_eq!(count, 1);
                assert_eq!(errors, "compilation failed");
            }
            _ => panic!("Expected Compilation error"),
        }
    }

    #[test]
    fn test_unwrap_compilation_result_with_filter() {
        let result = Warned {
            output: Ok(42),
            warnings: eco_vec![
                create_warning("html export is under active development"),
                create_warning("other warning"),
            ],
        };

        // Filter out HTML development warning
        let filter = |w: &SourceDiagnostic| {
            !w.message
                .contains("html export is under active development")
        };

        let output = unwrap_compilation_result(None, result, Some(filter));
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), 42);
    }

    #[test]
    fn test_handle_export_errors_pdf() {
        let errors = eco_vec![
            create_error("PDF export failed"),
            create_error("invalid document structure"),
        ];
        let result = handle_export_errors(errors, ExportErrorType::Pdf);

        match result {
            RheoError::PdfGeneration { count, errors } => {
                assert_eq!(count, 2);
                assert_eq!(errors, "PDF export failed\ninvalid document structure");
            }
            _ => panic!("Expected PdfGeneration error"),
        }
    }

    #[test]
    fn test_handle_export_errors_html() {
        let errors = eco_vec![create_error("HTML generation error")];
        let result = handle_export_errors(errors, ExportErrorType::Html);

        match result {
            RheoError::HtmlGeneration { count, errors } => {
                assert_eq!(count, 1);
                assert_eq!(errors, "HTML generation error");
            }
            _ => panic!("Expected HtmlGeneration error"),
        }
    }

    #[test]
    fn test_export_error_type_name() {
        assert_eq!(ExportErrorType::Pdf.name(), "PDF");
        assert_eq!(ExportErrorType::Html.name(), "HTML");
    }

    #[test]
    fn test_handle_typst_errors_empty() {
        let errors = eco_vec![];
        let result = handle_typst_errors(None, errors);

        match result {
            RheoError::Compilation { count, errors } => {
                assert_eq!(count, 0);
                assert_eq!(errors, "");
            }
            _ => panic!("Expected Compilation error"),
        }
    }

    #[test]
    fn test_handle_export_errors_pdf_single() {
        let errors = eco_vec![create_error("PDF error")];
        let result = handle_export_errors(errors, ExportErrorType::Pdf);

        match result {
            RheoError::PdfGeneration { count, errors } => {
                assert_eq!(count, 1);
                assert_eq!(errors, "PDF error");
            }
            _ => panic!("Expected PdfGeneration error"),
        }
    }

    #[test]
    fn test_handle_export_errors_html_multiple() {
        let errors = eco_vec![create_error("HTML error 1"), create_error("HTML error 2"),];
        let result = handle_export_errors(errors, ExportErrorType::Html);

        match result {
            RheoError::HtmlGeneration { count, errors } => {
                assert_eq!(count, 2);
                assert_eq!(errors, "HTML error 1\nHTML error 2");
            }
            _ => panic!("Expected HtmlGeneration error"),
        }
    }

    #[test]
    fn test_unwrap_compilation_result_multiple_warnings() {
        let result = Warned {
            output: Ok("success"),
            warnings: eco_vec![
                create_warning("warning 1"),
                create_warning("warning 2"),
                create_warning("warning 3"),
            ],
        };

        let output = unwrap_compilation_result(None, result, None::<fn(&SourceDiagnostic) -> bool>);
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), "success");
    }

    #[test]
    fn test_unwrap_compilation_result_filter_keeps_all() {
        let result = Warned {
            output: Ok(100),
            warnings: eco_vec![create_warning("keep this"), create_warning("keep that"),],
        };

        // Filter that keeps everything
        let filter = |_w: &SourceDiagnostic| true;

        let output = unwrap_compilation_result(None, result, Some(filter));
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), 100);
    }

    #[test]
    fn test_unwrap_compilation_result_filter_removes_all() {
        let result = Warned {
            output: Ok(200),
            warnings: eco_vec![create_warning("remove this"), create_warning("remove that"),],
        };

        // Filter that removes everything
        let filter = |_w: &SourceDiagnostic| false;

        let output = unwrap_compilation_result(None, result, Some(filter));
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), 200);
    }

    #[test]
    fn test_unwrap_compilation_result_no_warnings() {
        let result = Warned {
            output: Ok("no warnings"),
            warnings: eco_vec![],
        };

        let output = unwrap_compilation_result(None, result, None::<fn(&SourceDiagnostic) -> bool>);
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), "no warnings");
    }

    #[test]
    fn test_unwrap_compilation_result_multiple_errors() {
        let result: Warned<SourceResult<String>> = Warned {
            output: Err(eco_vec![
                create_error("error 1"),
                create_error("error 2"),
                create_error("error 3"),
            ]),
            warnings: eco_vec![create_warning("warning before errors")],
        };

        let output = unwrap_compilation_result(None, result, None::<fn(&SourceDiagnostic) -> bool>);
        assert!(output.is_err());

        match output.unwrap_err() {
            RheoError::Compilation { count, errors } => {
                assert_eq!(count, 3);
                assert!(errors.contains("error 1"));
                assert!(errors.contains("error 2"));
                assert!(errors.contains("error 3"));
            }
            _ => panic!("Expected Compilation error"),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use ecow::eco_vec;
    use typst::diag::{EcoString, Severity};
    use typst_syntax::Span;

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
        let result = handle_typst_errors(errors);

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
        let result = handle_typst_errors(errors);

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

        let output = unwrap_compilation_result(result, None::<fn(&SourceDiagnostic) -> bool>);
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), 42);
    }

    #[test]
    fn test_unwrap_compilation_result_error() {
        let result: Warned<SourceResult<i32>> = Warned {
            output: Err(eco_vec![create_error("compilation failed")]),
            warnings: eco_vec![],
        };

        let output = unwrap_compilation_result(result, None::<fn(&SourceDiagnostic) -> bool>);
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
            !w.message.contains("html export is under active development")
        };

        let output = unwrap_compilation_result(result, Some(filter));
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
        let result = handle_typst_errors(errors);

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
        let errors = eco_vec![
            create_error("HTML error 1"),
            create_error("HTML error 2"),
        ];
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

        let output = unwrap_compilation_result(result, None::<fn(&SourceDiagnostic) -> bool>);
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), "success");
    }

    #[test]
    fn test_unwrap_compilation_result_filter_keeps_all() {
        let result = Warned {
            output: Ok(100),
            warnings: eco_vec![
                create_warning("keep this"),
                create_warning("keep that"),
            ],
        };

        // Filter that keeps everything
        let filter = |_w: &SourceDiagnostic| true;

        let output = unwrap_compilation_result(result, Some(filter));
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), 100);
    }

    #[test]
    fn test_unwrap_compilation_result_filter_removes_all() {
        let result = Warned {
            output: Ok(200),
            warnings: eco_vec![
                create_warning("remove this"),
                create_warning("remove that"),
            ],
        };

        // Filter that removes everything
        let filter = |_w: &SourceDiagnostic| false;

        let output = unwrap_compilation_result(result, Some(filter));
        assert!(output.is_ok());
        assert_eq!(output.unwrap(), 200);
    }

    #[test]
    fn test_unwrap_compilation_result_no_warnings() {
        let result = Warned {
            output: Ok("no warnings"),
            warnings: eco_vec![],
        };

        let output = unwrap_compilation_result(result, None::<fn(&SourceDiagnostic) -> bool>);
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

        let output = unwrap_compilation_result(result, None::<fn(&SourceDiagnostic) -> bool>);
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

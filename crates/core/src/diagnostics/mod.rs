//! Turning Typst's diagnostics into rheo's, and nothing else.
//!
//! Rendering — where diagnostics go, in what colours, at what verbosity — is
//! the terminal-facing caller's business: core hands back a [`RheoError`] and,
//! for anything that is not an error, a [`DiagnosticReport`] the caller renders
//! when it chooses. A library user can therefore obtain an error without rheo
//! writing to their terminal behind their back.

pub mod error;
pub mod report;
pub mod results;

use crate::diagnostics::error::RheoError;
pub use report::{Diagnostic, DiagnosticReport, Severity, SourceFile, Span, TracePoint};

use ecow::EcoVec;
use typst::diag::SourceDiagnostic;

/// The messages of `diagnostics`, one per line — the plain form a [`RheoError`]
/// carries for its `Display`, alongside the structured report.
fn joined(diagnostics: &[SourceDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|d| d.message.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A Typst compilation failure as a [`RheoError::Compilation`].
pub fn compilation_error(errors: &EcoVec<SourceDiagnostic>) -> RheoError {
    RheoError::Compilation {
        count: errors.len(),
        errors: joined(errors),
    }
}

/// An export failure for `format` (`"PDF"`, `"HTML"`, …).
pub fn export_error(format: &'static str, errors: &EcoVec<SourceDiagnostic>) -> RheoError {
    RheoError::Export {
        format,
        count: errors.len(),
        errors: joined(errors),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ecow::eco_vec;
    use typst::diag::{EcoString, Severity, SourceDiagnostic};
    use typst::syntax::Span;

    fn create_error(message: &str) -> SourceDiagnostic {
        SourceDiagnostic {
            span: Span::detached().into(),
            message: EcoString::from(message),
            severity: Severity::Error,
            hints: eco_vec![],
            trace: eco_vec![],
        }
    }

    #[test]
    fn test_compilation_error_counts_and_joins_messages() {
        match compilation_error(&eco_vec![create_error("test error")]) {
            RheoError::Compilation { count, errors } => {
                assert_eq!(count, 1);
                assert_eq!(errors, "test error");
            }
            other => panic!("Expected Compilation error, got {other:?}"),
        }

        let errors = eco_vec![
            create_error("error 1"),
            create_error("error 2"),
            create_error("error 3"),
        ];
        match compilation_error(&errors) {
            RheoError::Compilation { count, errors } => {
                assert_eq!(count, 3);
                assert_eq!(errors, "error 1\nerror 2\nerror 3");
            }
            other => panic!("Expected Compilation error, got {other:?}"),
        }
    }

    #[test]
    fn test_compilation_error_empty() {
        match compilation_error(&eco_vec![]) {
            RheoError::Compilation { count, errors } => {
                assert_eq!(count, 0);
                assert_eq!(errors, "");
            }
            other => panic!("Expected Compilation error, got {other:?}"),
        }
    }

    /// One variant serves every format, tagged by name: a PDF failure and an
    /// HTML failure differ in their `format`, not in their shape.
    #[test]
    fn test_export_error_is_tagged_by_format() {
        let errors = eco_vec![
            create_error("PDF export failed"),
            create_error("invalid document structure"),
        ];
        match export_error("PDF", &errors) {
            RheoError::Export {
                format,
                count,
                errors,
            } => {
                assert_eq!(format, "PDF");
                assert_eq!(count, 2);
                assert_eq!(errors, "PDF export failed\ninvalid document structure");
            }
            other => panic!("Expected Export error, got {other:?}"),
        }

        match export_error("HTML", &eco_vec![create_error("HTML generation error")]) {
            RheoError::Export { format, count, .. } => {
                assert_eq!(format, "HTML");
                assert_eq!(count, 1);
            }
            other => panic!("Expected Export error, got {other:?}"),
        }
    }
}

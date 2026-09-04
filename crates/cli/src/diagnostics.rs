//! Rendering a compile's diagnostics for a terminal.
//!
//! Core resolves diagnostics into a [`DiagnosticReport`] and hands it back; the
//! destination (stderr), the colours and the source-context styling are decided
//! here, where the terminal actually is.

use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term::{self, termcolor::ColorChoice, termcolor::StandardStream};
use rheo_core::diagnostics::report::{DiagnosticReport, Severity, Span};

/// Write `report` to stderr with source context, in the Typst CLI's style.
///
/// Rendering is a courtesy: a failure to write diagnostics must not become a
/// second failure on top of whatever produced them, so it is dropped.
pub fn render(report: &DiagnosticReport) {
    if report.is_empty() {
        return;
    }

    // codespan's file ids are the insertion order, which is exactly how the
    // report's own `Span::file` indexes them.
    let mut files = SimpleFiles::new();
    for file in report.files() {
        files.add(file.name.clone(), file.text.clone());
    }

    let config = term::Config {
        tab_width: 2,
        ..Default::default()
    };
    let mut stderr = StandardStream::stderr(ColorChoice::Auto);
    let label = |span: &Option<Span>| {
        span.as_ref()
            .map(|s| Label::primary(s.file, s.range.clone()))
            .into_iter()
            .collect::<Vec<_>>()
    };

    for diagnostic in report.diagnostics() {
        let rendered = match diagnostic.severity {
            Severity::Error => Diagnostic::error(),
            Severity::Warning => Diagnostic::warning(),
        }
        .with_message(diagnostic.message.clone())
        .with_notes(
            diagnostic
                .hints
                .iter()
                .map(|hint| format!("hint: {hint}"))
                .collect(),
        )
        .with_labels(label(&diagnostic.span));

        let _ = term::emit_to_write_style(&mut stderr, &config, &files, &rendered);

        // Typst's stack-trace equivalent, rendered as trailing help notes.
        for point in &diagnostic.trace {
            let help = Diagnostic::help()
                .with_message(point.message.clone())
                .with_labels(label(&point.span));
            let _ = term::emit_to_write_style(&mut stderr, &config, &files, &help);
        }
    }
}

//! Rendering a compile's diagnostics for a terminal.
//!
//! Core resolves diagnostics into a [`DiagnosticReport`] and hands it back; the
//! destination (stderr), the colours, the source-context styling, and the
//! wording of the note pointing a `generated` primary span at
//! `--emit-bundle-source` are decided here, where the terminal actually is.

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
        let mut notes: Vec<String> = diagnostic
            .hints
            .iter()
            .map(|hint| format!("hint: {hint}"))
            .collect();
        // Emitted once, only for the primary span, and only when it lands in
        // Typst rheo generated — the trace's own points already read as a
        // chain, where a repeated note per point would be noise.
        if diagnostic
            .span
            .as_ref()
            .is_some_and(|s| report.files()[s.file].generated)
        {
            notes.push(
                "note: this location is in Typst rheo generated, not a file in your project; \
                 run with --emit-bundle-source to write the synthesized source to \
                 <build_dir>/<format>/.rheo-bundle.typ and read it there"
                    .to_string(),
            );
        }

        let rendered = match diagnostic.severity {
            Severity::Error => Diagnostic::error(),
            Severity::Warning => Diagnostic::warning(),
        }
        .with_message(diagnostic.message.clone())
        .with_notes(notes)
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

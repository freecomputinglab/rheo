//! Diagnostics detached from the compile that produced them.
//!
//! Typst reports a diagnostic as a span into a world's files, so rendering one
//! needs that world alive. A [`DiagnosticReport`] resolves the spans up front
//! and keeps a copy of every file they point into, so the terminal-facing side
//! of rheo — which owns the destination, the colours and the verbosity — can
//! render a compile's diagnostics long after the world is gone. Core never
//! writes them anywhere itself.

use crate::world::RheoWorld;
use codespan_reporting::files::Files;
use std::ops::Range;
use typst::WorldExt;
use typst::diag::SourceDiagnostic;

/// How loud one diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A source file a diagnostic points into, carried along so a renderer can
/// show the offending lines.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Display name, project-relative (or `@pkg/name:ver/path` for a package).
    pub name: String,
    pub text: String,
}

/// A byte range in one of the report's [`SourceFile`]s.
#[derive(Debug, Clone)]
pub struct Span {
    /// Index into [`DiagnosticReport::files`].
    pub file: usize,
    pub range: Range<usize>,
}

/// One step of a diagnostic's trace — Typst's stack-trace equivalent.
#[derive(Debug, Clone)]
pub struct TracePoint {
    pub message: String,
    pub span: Option<Span>,
}

/// One diagnostic: what went wrong, where, and what to try instead.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub hints: Vec<String>,
    pub span: Option<Span>,
    pub trace: Vec<TracePoint>,
}

/// Everything one or more compiles reported, plus the sources needed to render
/// it.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticReport {
    files: Vec<SourceFile>,
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticReport {
    /// Resolve `diagnostics` against the world that produced them.
    pub fn detach(world: &RheoWorld, diagnostics: &[SourceDiagnostic]) -> Self {
        let mut report = Self::default();
        for diagnostic in diagnostics {
            let entry = Diagnostic {
                severity: match diagnostic.severity {
                    typst::diag::Severity::Error => Severity::Error,
                    typst::diag::Severity::Warning => Severity::Warning,
                },
                message: diagnostic.message.to_string(),
                hints: diagnostic.hints.iter().map(|h| h.v.to_string()).collect(),
                span: report.resolve(world, diagnostic.span),
                trace: diagnostic
                    .trace
                    .iter()
                    .map(|point| TracePoint {
                        message: point.v.to_string(),
                        span: report.resolve(world, point.span.into()),
                    })
                    .collect(),
            };
            report.diagnostics.push(entry);
        }
        report
    }

    /// The files diagnostics point into, in the order [`Span::file`] indexes them.
    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// Absorb `other`, remapping its spans onto this report's file list.
    pub fn extend(&mut self, other: Self) {
        let remap: Vec<usize> = other.files.into_iter().map(|f| self.intern(f)).collect();
        let shift = |span: Option<Span>| {
            span.map(|span| Span {
                file: remap[span.file],
                range: span.range,
            })
        };
        for mut diagnostic in other.diagnostics {
            diagnostic.span = shift(diagnostic.span);
            diagnostic.trace = diagnostic
                .trace
                .into_iter()
                .map(|point| TracePoint {
                    message: point.message,
                    span: shift(point.span),
                })
                .collect();
            self.diagnostics.push(diagnostic);
        }
    }

    /// Resolve a Typst span into this report's own file list, keeping the file's
    /// text. A detached span (or a file the world cannot serve) resolves to
    /// `None`, and the diagnostic renders without source context.
    fn resolve(&mut self, world: &RheoWorld, span: typst::syntax::DiagSpan) -> Option<Span> {
        let id = span.id()?;
        let range = world.range(span)?;
        let name = world.name(id).ok()?;
        let text = world.source(id).ok()?.text().to_string();
        Some(Span {
            file: self.intern(SourceFile { name, text }),
            range,
        })
    }

    /// The index of `file` in this report, adding it if it is new. Files are
    /// identified by name: one compile serves one text per name.
    fn intern(&mut self, file: SourceFile) -> usize {
        match self.files.iter().position(|f| f.name == file.name) {
            Some(index) => index,
            None => {
                self.files.push(file);
                self.files.len() - 1
            }
        }
    }
}

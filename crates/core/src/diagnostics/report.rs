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
    /// True when this is Typst rheo generated — the synthesized bundle main, a
    /// per-vertebra injected prelude, or one of rheo's own served modules — and
    /// so not a file the project can open and edit.
    pub generated: bool,
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
    ///
    /// The world's source map translates the span's synthesized-text range
    /// back to the authored file it came from when it can; a span that
    /// genuinely lands in Typst rheo generated keeps today's synthesized
    /// text and range, interned under a name distinct from the authored
    /// file's own so the two never share (and corrupt) one entry.
    fn resolve(&mut self, world: &RheoWorld, span: typst::syntax::DiagSpan) -> Option<Span> {
        let id = span.id()?;
        let range = world.range(span)?;
        let name = world.name(id).ok()?;
        match world.source_map(id).resolve(&range) {
            Some((file, authored_range)) => Some(Span {
                file: self.intern(SourceFile {
                    name: file.name.clone(),
                    text: file.text.to_string(),
                    generated: false,
                }),
                range: authored_range,
            }),
            None => {
                let text = world.source(id).ok()?.text().to_string();
                Some(Span {
                    file: self.intern(SourceFile {
                        name: format!("{name} (rheo-generated)"),
                        text,
                        generated: true,
                    }),
                    range,
                })
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::constants::METADATA_MODULE_PATH;
    use crate::world::WorldSpec;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::TempDir;
    use typst::World;
    use typst::syntax::{DiagSpan, RootedPath, VirtualPath, VirtualRoot};

    /// A span into a vertebra's own served text resolves authored
    /// (`generated: false`); a span into a served-from-memory rheo module —
    /// entirely injected, per its `SourceMap` — resolves generated.
    #[test]
    fn resolve_marks_generated_only_for_a_synthesized_only_source() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let mut source_overlay = HashMap::new();
        source_overlay.insert("content/a.typ".to_string(), "= Title\n".to_string());

        let world = RheoWorld::new_for_bundle(
            root,
            "#document(\"a.html\", format: \"html\")[#include \"content/a.typ\"]".to_string(),
            WorldSpec {
                source_overlay: Arc::new(source_overlay),
                format_name: Some("html".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let vertebra_id = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("content/a.typ").unwrap(),
        )
        .intern();
        let text = World::source(&world, vertebra_id)
            .unwrap()
            .text()
            .to_string();
        let offset = text.find("Title").unwrap();
        let span = DiagSpan::from_range(vertebra_id, offset..offset + "Title".len());

        let mut report = DiagnosticReport::default();
        let resolved = report.resolve(&world, span).expect("resolves");
        assert!(!report.files()[resolved.file].generated);

        let metadata_id = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new(METADATA_MODULE_PATH).unwrap(),
        )
        .intern();
        World::source(&world, metadata_id).unwrap();
        let span = DiagSpan::from_range(metadata_id, 0..1);
        let resolved = report.resolve(&world, span).expect("resolves");
        assert!(report.files()[resolved.file].generated);
    }
}

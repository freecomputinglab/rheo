//! The user-facing output channel.
//!
//! Everything rheo says to the person who ran it goes through a [`Reporter`];
//! `tracing` stays the developer's channel, carrying structured fields for
//! `RUST_LOG=rheo=info`. A message that belongs to both is spelled once here,
//! rather than as a `println!` and an `info!` kept in step by hand.
//!
//! Writing through a writer also makes the output assertable: a test builds a
//! [`Reporter::capture`] and reads back exactly what the user would have seen.

use std::io::Write;
use std::path::Path;
use tracing::info;

/// One command's user-facing output.
pub struct Reporter {
    out: Box<dyn Write + Send>,
}

impl Reporter {
    /// The ordinary reporter: writes to stdout.
    pub fn stdout() -> Self {
        Self {
            out: Box::new(std::io::stdout()),
        }
    }

    /// A reporter writing into a buffer, plus the handle to read it back.
    #[cfg(test)]
    pub fn capture() -> (Self, Capture) {
        let capture = Capture::default();
        (
            Self {
                out: Box::new(capture.clone()),
            },
            capture,
        )
    }

    /// One line of output.
    pub fn line(&mut self, text: impl std::fmt::Display) {
        self.write(format_args!("{text}\n"));
    }

    /// A rewrite made at one place in one file: reported as
    /// `<path>:<line>: <old>  ->  <new>`, and logged with the same values as
    /// tracing fields.
    pub fn rewrite(
        &mut self,
        path: &Path,
        line: usize,
        old: impl std::fmt::Display,
        new: impl std::fmt::Display,
    ) {
        info!(file = %path.display(), line, old = %old, new = %new, "rewrite");
        self.line(format_args!("{}:{line}: {old}  ->  {new}", path.display()));
    }

    /// Something done to a file as a whole (no line to point at), reported as
    /// `<path>: <label>` and logged with the same values.
    pub fn note(&mut self, path: &Path, label: impl std::fmt::Display) {
        info!(file = %path.display(), label = %label, "migrate");
        self.line(format_args!("{}: {label}", path.display()));
    }

    /// A retired name found at `location`, and what replaces it.
    pub fn retired(&mut self, location: &str, name: &str, replacement: &str) {
        self.line(format_args!("{location}: `{name}` — {replacement}"));
    }

    /// Output is a courtesy, never the work: a closed pipe must not fail a
    /// migration halfway through, so a write error is dropped rather than
    /// propagated.
    fn write(&mut self, args: std::fmt::Arguments<'_>) {
        let _ = self.out.write_fmt(args);
    }
}

/// A buffer a captured reporter writes into, shared with the test that reads
/// it back.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct Capture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

#[cfg(test)]
impl Capture {
    /// Everything written so far, as text.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("capture buffer poisoned")).into_owned()
    }
}

#[cfg(test)]
impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("capture buffer poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_reads_back_every_line() {
        let (mut reporter, captured) = Reporter::capture();
        reporter.line("plain");
        reporter.rewrite(Path::new("content/a.typ"), 12, "old", "new");
        reporter.note(Path::new("content/b.typ"), "shimmed");
        reporter.retired("[html]", "feed_title", "moved to @rheo/feeds");

        assert_eq!(
            captured.text(),
            "plain\n\
             content/a.typ:12: old  ->  new\n\
             content/b.typ: shimmed\n\
             [html]: `feed_title` — moved to @rheo/feeds\n"
        );
    }
}

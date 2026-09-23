//! Reads Typst's introspection-convergence iterations off `typst_timing`
//! without ever materializing the trace.
//!
//! `typst::compile` re-runs layout until introspected state (e.g. counters
//! read before they're set) stabilizes, up to `MAX_ITERS = 5`, and each pass
//! is a `TimingScope` named `iter (N)`. `typst_timing` exposes no in-memory
//! reader for its collected events — only `export_json`, which serializes
//! every event as Chrome-tracing JSON to a `Write` and clears its buffer.
//! [`ConvergenceScanner`] is that `Write`: it scans the byte stream for
//! `iter (N)` `B`/`E` pairs and discards everything else, so a multi-MB
//! trace never exists in memory or on disk unless `--timings` also wants a
//! copy (see [`ConvergenceScanner::with_tee`]).

use std::io::{self, Write};
use std::time::Duration;

use super::timing::human_duration;

/// `typst_timing::export_json`'s `Entry` struct serializes its fields in
/// this fixed order (`name`, `cat`, `ph`, `ts`, ...), so each `iter (N)`
/// event's `ph`/`ts` can be found by matching these literal byte runs in
/// sequence rather than parsing full JSON objects.
const PREFIX: &[u8] = br#""name":"iter ("#;
const MID: &[u8] = br#")","cat":"typst","ph":""#;
const TS_LIT: &[u8] = br#"","ts":"#;

/// One byte-at-a-time state machine, so correctness never depends on where
/// `write` splits its input.
enum State {
    Searching,
    MatchingPrefix(usize),
    ReadingIterNum(String),
    MatchingMid(usize, u32),
    ReadingPh(u32),
    MatchingTsLit(usize, u32, u8),
    ReadingTs(u32, u8, String),
}

/// Scans a `typst_timing::export_json` byte stream for `iter (N)` events,
/// optionally teeing every byte to a second `Write` (the `--timings` trace
/// file, when both flags are given — `export_json` clears its buffer on
/// export, so there can only be one export per compile).
pub struct ConvergenceScanner {
    tee: Option<Box<dyn Write>>,
    state: State,
    /// `(iteration, ph, timestamp_micros)`, in arrival order.
    events: Vec<(u32, u8, f64)>,
}

impl Default for ConvergenceScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ConvergenceScanner {
    pub fn new() -> Self {
        Self {
            tee: None,
            state: State::Searching,
            events: Vec::new(),
        }
    }

    pub fn with_tee(tee: Box<dyn Write>) -> Self {
        Self {
            tee: Some(tee),
            ..Self::new()
        }
    }

    /// Advances the state machine by one byte. Takes `self.state` by value
    /// (via [`std::mem::replace`], leaving `Searching` in its place) so a
    /// mismatch can recurse into a fresh [`Self::feed_byte`] call on the same
    /// byte — a naive restart, not full KMP, which is fine for a handful of
    /// fixed non-self-overlapping literals — without fighting the borrow
    /// checker over a `match &mut self.state` still active in the arm body.
    fn feed_byte(&mut self, b: u8) {
        match std::mem::replace(&mut self.state, State::Searching) {
            State::Searching => {
                if b == PREFIX[0] {
                    self.state = State::MatchingPrefix(1);
                }
            }
            State::MatchingPrefix(idx) => {
                if b == PREFIX[idx] {
                    self.state = if idx + 1 == PREFIX.len() {
                        State::ReadingIterNum(String::new())
                    } else {
                        State::MatchingPrefix(idx + 1)
                    };
                } else {
                    self.feed_byte(b);
                }
            }
            State::ReadingIterNum(mut digits) => {
                if b.is_ascii_digit() {
                    digits.push(b as char);
                    self.state = State::ReadingIterNum(digits);
                } else if b == b')' {
                    let n = digits.parse().unwrap_or(0);
                    self.state = State::MatchingMid(1, n);
                }
                // Anything else is malformed for this literal; abort to
                // `Searching` (already the default from the `replace` above).
            }
            State::MatchingMid(idx, n) => {
                if b == MID[idx] {
                    self.state = if idx + 1 == MID.len() {
                        State::ReadingPh(n)
                    } else {
                        State::MatchingMid(idx + 1, n)
                    };
                } else {
                    self.feed_byte(b);
                }
            }
            State::ReadingPh(n) => {
                self.state = State::MatchingTsLit(0, n, b);
            }
            State::MatchingTsLit(idx, n, ph) => {
                if b == TS_LIT[idx] {
                    self.state = if idx + 1 == TS_LIT.len() {
                        State::ReadingTs(n, ph, String::new())
                    } else {
                        State::MatchingTsLit(idx + 1, n, ph)
                    };
                } else {
                    self.feed_byte(b);
                }
            }
            State::ReadingTs(n, ph, mut digits) => {
                if b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E') {
                    digits.push(b as char);
                    self.state = State::ReadingTs(n, ph, digits);
                } else {
                    if let Ok(ts) = digits.parse::<f64>() {
                        self.events.push((n, ph, ts));
                    }
                    self.feed_byte(b);
                }
            }
        }
    }

    /// Pairs each iteration's `B`/`E` timestamps and formats the one-line
    /// report, e.g. `5 convergence iteration(s) — iter(1) 1.3s, iter(2)
    /// 590ms, ...`. `None` when no iteration events were seen at all.
    pub fn finish(self) -> Option<String> {
        let mut open: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();
        let mut durations: Vec<(u32, Duration)> = Vec::new();
        for (n, ph, ts) in self.events {
            match ph {
                b'B' => {
                    open.insert(n, ts);
                }
                b'E' => {
                    if let Some(start) = open.remove(&n) {
                        let micros = (ts - start).max(0.0);
                        durations.push((n, Duration::from_micros(micros as u64)));
                    }
                }
                _ => {}
            }
        }
        if durations.is_empty() {
            return None;
        }
        durations.sort_by_key(|(n, _)| *n);
        let parts: Vec<String> = durations
            .iter()
            .map(|(n, d)| format!("iter({n}) {}", human_duration(*d)))
            .collect();
        Some(format!(
            "{} convergence iteration(s) — {}",
            durations.len(),
            parts.join(", ")
        ))
    }
}

impl Write for ConvergenceScanner {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(tee) = &mut self.tee {
            tee.write_all(buf)?;
        }
        for &b in buf {
            self.feed_byte(b);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.tee {
            Some(tee) => tee.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic `export_json` trace: two `iter (N)` pairs with an
    /// unrelated event interleaved, plus an `args` object (itself carrying
    /// nested braces) to prove the scanner doesn't need to track object
    /// boundaries at all.
    fn synthetic_trace() -> String {
        concat!(
            r#"[{"name":"iter (1)","cat":"typst","ph":"B","ts":0.0,"pid":1,"tid":1,"args":null},"#,
            r#"{"name":"layout","cat":"typst","ph":"B","ts":10.0,"pid":1,"tid":1,"args":{"file":"a.typ","line":3}},"#,
            r#"{"name":"layout","cat":"typst","ph":"E","ts":500.0,"pid":1,"tid":1,"args":{"file":"a.typ","line":3}},"#,
            r#"{"name":"iter (1)","cat":"typst","ph":"E","ts":1300000.0,"pid":1,"tid":1,"args":null},"#,
            r#"{"name":"iter (2)","cat":"typst","ph":"B","ts":1300000.0,"pid":1,"tid":1,"args":null},"#,
            r#"{"name":"iter (2)","cat":"typst","ph":"E","ts":1890000.0,"pid":1,"tid":1,"args":null}]"#,
        )
        .to_string()
    }

    #[test]
    fn scans_whole_buffer_in_one_write() {
        let mut scanner = ConvergenceScanner::new();
        scanner.write_all(synthetic_trace().as_bytes()).unwrap();
        let line = scanner.finish().unwrap();
        assert_eq!(
            line,
            "2 convergence iteration(s) — iter(1) 1.3s, iter(2) 590ms"
        );
    }

    #[test]
    fn scans_identically_one_byte_at_a_time() {
        let mut scanner = ConvergenceScanner::new();
        for b in synthetic_trace().as_bytes() {
            scanner.write_all(&[*b]).unwrap();
        }
        let line = scanner.finish().unwrap();
        assert_eq!(
            line,
            "2 convergence iteration(s) — iter(1) 1.3s, iter(2) 590ms"
        );
    }

    #[test]
    fn returns_none_when_no_iterations_seen() {
        let mut scanner = ConvergenceScanner::new();
        scanner
            .write_all(br#"[{"name":"layout","cat":"typst","ph":"B","ts":0.0,"pid":1,"tid":1,"args":null}]"#)
            .unwrap();
        assert!(scanner.finish().is_none());
    }

    /// A `Write` sink two owners can inspect: the scanner writes through its
    /// boxed half while the test still holds the other, standing in for the
    /// `--timings` trace file's `BufWriter<File>`.
    #[derive(Clone, Default)]
    struct SharedBuf(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

    impl Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn tees_every_byte_to_the_second_writer() {
        let tee = SharedBuf::default();
        let mut scanner = ConvergenceScanner::with_tee(Box::new(tee.clone()));
        scanner.write_all(synthetic_trace().as_bytes()).unwrap();
        assert!(scanner.finish().is_some());
        assert_eq!(*tee.0.borrow(), synthetic_trace().as_bytes());
    }

    #[test]
    fn single_converged_iteration_still_reports() {
        let mut scanner = ConvergenceScanner::new();
        scanner
            .write_all(
                br#"[{"name":"iter (1)","cat":"typst","ph":"B","ts":0.0,"pid":1,"tid":1,"args":null},"#,
            )
            .unwrap();
        scanner
            .write_all(
                br#"{"name":"iter (1)","cat":"typst","ph":"E","ts":250.0,"pid":1,"tid":1,"args":null}]"#,
            )
            .unwrap();
        assert_eq!(
            scanner.finish().unwrap(),
            "1 convergence iteration(s) — iter(1) 0ms"
        );
    }
}

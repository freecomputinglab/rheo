//! Structured watch-session events, appended as JSON lines to the file named
//! by `rheo watch --events`.

use rheo_core::{Result, RheoError};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    Full,
}

impl BuildKind {
    fn as_str(self) -> &'static str {
        match self {
            BuildKind::Full => "full",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchReport {
    ServerReady {
        url: String,
    },
    BuildFinished {
        initial: bool,
        ok: bool,
        kind: BuildKind,
        ms: u128,
        error: Option<String>,
    },
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

impl WatchReport {
    pub fn to_json_line(&self) -> String {
        match self {
            WatchReport::ServerReady { url } => {
                format!(r#"{{"event":"server_ready","url":"{}"}}"#, escape(url))
            }
            WatchReport::BuildFinished {
                initial,
                ok,
                kind,
                ms,
                error,
            } => {
                let mut line = format!(
                    r#"{{"event":"build_finished","initial":{initial},"ok":{ok},"kind":"{}","ms":{ms}"#,
                    kind.as_str()
                );
                if let Some(e) = error {
                    line.push_str(&format!(r#","error":"{}""#, escape(e)));
                }
                line.push('}');
                line
            }
        }
    }
}

/// Where watch events go; a no-op when `--events` was not given.
pub struct EventSink(Option<File>);

impl EventSink {
    pub fn open(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(Self(None));
        };
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| RheoError::io(e, format!("opening events file {}", path.display())))?;
        Ok(Self(Some(file)))
    }

    /// Best-effort: a failed write must not take down the watch session.
    pub fn emit(&mut self, report: &WatchReport) {
        let Some(file) = &mut self.0 else { return };
        let line = format!("{}\n", report.to_json_line());
        if let Err(e) = file.write_all(line.as_bytes()).and_then(|_| file.flush()) {
            tracing::warn!(error = %e, "failed to write watch event");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(initial: bool, ok: bool, ms: u128, error: Option<&str>) -> WatchReport {
        WatchReport::BuildFinished {
            initial,
            ok,
            kind: BuildKind::Full,
            ms,
            error: error.map(String::from),
        }
    }

    #[test]
    fn line_shapes() {
        assert_eq!(
            WatchReport::ServerReady {
                url: "http://localhost:3001".into()
            }
            .to_json_line(),
            r#"{"event":"server_ready","url":"http://localhost:3001"}"#
        );
        assert_eq!(
            finished(true, true, 1234, None).to_json_line(),
            r#"{"event":"build_finished","initial":true,"ok":true,"kind":"full","ms":1234}"#
        );
        assert_eq!(
            finished(false, false, 87, Some("boom")).to_json_line(),
            r#"{"event":"build_finished","initial":false,"ok":false,"kind":"full","ms":87,"error":"boom"}"#
        );
    }

    #[test]
    fn escapes_quotes_and_backslashes() {
        assert_eq!(
            WatchReport::ServerReady {
                url: r#"a"b\c"#.into()
            }
            .to_json_line(),
            r#"{"event":"server_ready","url":"a\"b\\c"}"#
        );
        assert_eq!(
            finished(false, false, 1, Some("x\"y\\z\nw")).to_json_line(),
            r#"{"event":"build_finished","initial":false,"ok":false,"kind":"full","ms":1,"error":"x\"y\\z\nw"}"#
        );
    }

    #[test]
    fn sink_appends_one_line_per_event() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ev.jsonl");
        let mut sink = EventSink::open(Some(&path)).unwrap();
        sink.emit(&WatchReport::ServerReady { url: "u".into() });
        sink.emit(&finished(true, true, 5, None));
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn sink_without_path_is_noop() {
        let mut sink = EventSink::open(None).unwrap();
        sink.emit(&WatchReport::ServerReady { url: "u".into() });
    }
}

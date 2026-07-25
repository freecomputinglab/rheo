//! A minimal Typst value model for synthesizing literals to inject into source.
//!
//! rheo injects small data structures into the compiled bundle (e.g. the
//! per-vertebra `rheo-context`). [`TypstLiteral`] models the value and
//! serializes it to a Typst literal, so call sites build a typed structure
//! rather than hand-assembling escaped strings.

/// A Typst value that serializes to a Typst literal.
///
/// Covers the shapes rheo needs to inject: strings, arrays, and dictionaries
/// (with identifier keys). Nest freely.
#[derive(Debug, Clone, PartialEq)]
pub enum TypstLiteral {
    /// A string literal (escaped on serialize).
    Str(String),
    /// A boolean literal (`true`/`false`).
    Bool(bool),
    /// The `none` literal.
    None,
    /// An array literal, e.g. `(a, b,)`.
    Array(Vec<TypstLiteral>),
    /// A dictionary literal with identifier keys, e.g. `(k: v)`.
    Dict(Vec<(String, TypstLiteral)>),
    /// A `datetime(...)` literal.
    Datetime {
        year: i32,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8,
    },
}

impl TypstLiteral {
    /// A string value from anything string-like.
    pub fn str(s: impl Into<String>) -> Self {
        TypstLiteral::Str(s.into())
    }

    /// A boolean value.
    pub fn bool(b: bool) -> Self {
        TypstLiteral::Bool(b)
    }

    /// Serialize to a Typst literal.
    ///
    /// Empty array serializes to `()`, empty dictionary to `(:)`. String values
    /// are escaped. Dictionary keys are emitted verbatim as identifiers, so keys
    /// must be valid Typst identifiers.
    pub fn serialize(&self) -> String {
        match self {
            TypstLiteral::Str(s) => serialize_string(s),
            TypstLiteral::Bool(b) => b.to_string(),
            TypstLiteral::None => "none".to_string(),
            TypstLiteral::Array(items) if items.is_empty() => "()".to_string(),
            TypstLiteral::Array(items) => {
                let inner: Vec<String> = items.iter().map(TypstLiteral::serialize).collect();
                format!("({},)", inner.join(", "))
            }
            TypstLiteral::Dict(pairs) if pairs.is_empty() => "(:)".to_string(),
            TypstLiteral::Dict(pairs) => {
                let inner: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.serialize()))
                    .collect();
                format!("({})", inner.join(", "))
            }
            TypstLiteral::Datetime {
                year,
                month,
                day,
                hour,
                minute,
                second,
            } => format!(
                "datetime(year: {year}, month: {month}, day: {day}, hour: {hour}, minute: {minute}, second: {second})"
            ),
        }
    }

    /// Convert to a runtime typst [`Value`] for programmatic APIs (e.g. `sys.inputs`),
    /// mirroring the shape that [`serialize`](Self::serialize) renders as source text.
    pub fn to_value(&self) -> typst::foundations::Value {
        use typst::foundations::{Array, Dict, Value};
        match self {
            TypstLiteral::Str(s) => Value::Str(s.as_str().into()),
            TypstLiteral::Bool(b) => Value::Bool(*b),
            TypstLiteral::None => Value::None,
            TypstLiteral::Array(items) => {
                Value::Array(items.iter().map(TypstLiteral::to_value).collect::<Array>())
            }
            TypstLiteral::Dict(pairs) => {
                let mut dict = Dict::new();
                for (k, v) in pairs {
                    dict.insert(k.as_str().into(), v.to_value());
                }
                Value::Dict(dict)
            }
            TypstLiteral::Datetime {
                year,
                month,
                day,
                hour,
                minute,
                second,
            } => {
                let dt = typst::foundations::Datetime::from_ymd_hms(
                    *year, *month, *day, *hour, *minute, *second,
                )
                .unwrap_or_else(|| {
                    typst::foundations::Datetime::from_ymd_hms(1970, 1, 1, 0, 0, 0)
                        .expect("1970-01-01 00:00:00 is always a valid datetime")
                });
                Value::Datetime(dt)
            }
        }
    }
}

/// Render `s` as a Typst string literal, escaping quotes/backslashes/control chars.
fn serialize_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_escapes_quotes_and_backslashes() {
        assert_eq!(TypstLiteral::str("a\"b\\c").serialize(), "\"a\\\"b\\\\c\"");
        assert_eq!(
            TypstLiteral::str("line\nbreak").serialize(),
            "\"line\\nbreak\""
        );
    }

    #[test]
    fn empty_array_and_dict_use_typst_syntax() {
        assert_eq!(TypstLiteral::Array(vec![]).serialize(), "()");
        assert_eq!(TypstLiteral::Dict(vec![]).serialize(), "(:)");
    }

    #[test]
    fn bool_serializes_and_converts_to_value_bool() {
        use typst::foundations::Value;
        assert_eq!(TypstLiteral::bool(true).serialize(), "true");
        assert_eq!(TypstLiteral::bool(false).serialize(), "false");
        assert_eq!(TypstLiteral::bool(true).to_value(), Value::Bool(true));
    }

    #[test]
    fn none_serializes_and_converts_to_value_none() {
        assert_eq!(TypstLiteral::None.serialize(), "none");
        assert_eq!(
            TypstLiteral::None.to_value(),
            typst::foundations::Value::None
        );
    }

    #[test]
    fn nested_dict_and_array_round_trip_to_literal() {
        let data = TypstLiteral::Dict(vec![
            ("handle".to_string(), TypstLiteral::str("chapters:intro")),
            (
                "spine".to_string(),
                TypstLiteral::Array(vec![TypstLiteral::Dict(vec![
                    ("handle".to_string(), TypstLiteral::str("intro")),
                    ("path".to_string(), TypstLiteral::str("content/intro.typ")),
                    ("title".to_string(), TypstLiteral::str("Introduction")),
                ])]),
            ),
        ]);
        assert_eq!(
            data.serialize(),
            "(handle: \"chapters:intro\", spine: ((handle: \"intro\", path: \"content/intro.typ\", title: \"Introduction\"),))"
        );
    }

    #[test]
    fn datetime_serializes_to_datetime_call() {
        let dt = TypstLiteral::Datetime {
            year: 2025,
            month: 3,
            day: 9,
            hour: 14,
            minute: 30,
            second: 5,
        };
        assert_eq!(
            dt.serialize(),
            "datetime(year: 2025, month: 3, day: 9, hour: 14, minute: 30, second: 5)"
        );
    }

    #[test]
    fn datetime_to_value_builds_value_datetime() {
        use typst::foundations::Value;
        let dt = TypstLiteral::Datetime {
            year: 2025,
            month: 3,
            day: 9,
            hour: 14,
            minute: 30,
            second: 5,
        };
        let Value::Datetime(_) = dt.to_value() else {
            panic!("expected Value::Datetime");
        };
    }

    #[test]
    fn to_value_builds_nested_dict() {
        use typst::foundations::Value;

        let data = TypstLiteral::Dict(vec![
            ("handle".to_string(), TypstLiteral::str("chapters:intro")),
            (
                "spine".to_string(),
                TypstLiteral::Array(vec![TypstLiteral::Dict(vec![
                    ("handle".to_string(), TypstLiteral::str("intro")),
                    ("path".to_string(), TypstLiteral::str("content/intro.typ")),
                    ("title".to_string(), TypstLiteral::str("Introduction")),
                ])]),
            ),
        ]);

        let value = data.to_value();
        let Value::Dict(dict) = value else {
            panic!("expected Value::Dict");
        };
        assert!(dict.at("handle".into(), None).is_ok());
        assert!(dict.at("spine".into(), None).is_ok());
    }
}

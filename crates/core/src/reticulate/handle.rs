//! [`Handle`]: the canonical, `:`-joined identifier for one vertebra.
//!
//! A handle is the crate's central cross-file identifier: a Typst label name,
//! a cross-vertebra `@handle` anchor, an on-disk output-path stem, and the key
//! under which a vertebra's metadata beacon publishes itself
//! (`rheo-meta:<handle>`). `src/typ/rheo.typ` parses one back apart with
//! `.split(":")` for link-depth arithmetic, so the `:` separator and the
//! `<handle.typ>` escape spelling are a cross-file contract, not an
//! implementation detail free to change here.

use crate::RESERVED_META_LABEL_PREFIX;
use std::fmt;

/// A vertebra's canonical handle (e.g. `chapters:intro`).
///
/// Built from disk-derived segments via [`Handle::root`]/[`Handle::child`],
/// which sanitize each segment on the way in — a `Handle` cannot hold a raw,
/// unsanitized directory/file name. [`Handle::new`] wraps an already-valid
/// handle string verbatim (no sanitization), for reassembling a value whose
/// pieces were sanitized earlier, or a literal already known to be valid.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Handle(String);

impl Handle {
    /// Wrap an already-valid handle string verbatim — no sanitization.
    pub fn new(s: impl Into<String>) -> Self {
        Handle(s.into())
    }

    /// Sanitize one path segment: keep alphanumeric, `-`, `_`; replace
    /// everything else with `_`. Safe for use in a Typst label.
    pub fn sanitize_segment(s: &str) -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// A bare, root-level handle built from one on-disk segment (sanitized).
    pub fn root(segment: &str) -> Self {
        Handle(Self::sanitize_segment(segment))
    }

    /// Append a `:`-joined child segment (sanitized).
    pub fn child(&self, segment: &str) -> Self {
        Handle(format!("{}:{}", self.0, Self::sanitize_segment(segment)))
    }

    /// Borrow the raw handle string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `<handle.typ>` escape-alias form.
    pub fn escape(&self) -> String {
        format!("{}.typ", self.0)
    }

    /// The on-disk output path for this handle under `ext`: `:` nesting
    /// translates back to `/` (a valid Typst label char; `/` is not), so
    /// nested vertebrae land in on-disk subdirectories.
    pub fn output_path(&self, ext: &str) -> String {
        format!("{}.{ext}", self.0.replace(':', "/"))
    }

    /// The `rheo-meta:<handle>` beacon label this vertebra publishes.
    pub fn meta_label(&self) -> String {
        format!("{RESERVED_META_LABEL_PREFIX}{}", self.0)
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Handle {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for Handle {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for Handle {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for Handle {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl From<&str> for Handle {
    fn from(s: &str) -> Self {
        Handle(s.to_string())
    }
}

impl From<String> for Handle {
    fn from(s: String) -> Self {
        Handle(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_sanitizes_segment() {
        assert_eq!(Handle::root("a b/c").as_str(), "a_b_c");
    }

    #[test]
    fn child_joins_with_colon_and_sanitizes() {
        let h = Handle::root("chapters").child("intro!");
        assert_eq!(h.as_str(), "chapters:intro_");
    }

    #[test]
    fn escape_appends_typ_suffix() {
        assert_eq!(Handle::root("intro").escape(), "intro.typ");
    }

    #[test]
    fn output_path_translates_colons_to_slashes() {
        let h = Handle::root("chapters").child("intro");
        assert_eq!(h.output_path("html"), "chapters/intro.html");
    }

    #[test]
    fn meta_label_uses_reserved_prefix() {
        let h = Handle::root("chapters").child("intro");
        assert_eq!(h.meta_label(), "rheo-meta:chapters:intro");
    }

    #[test]
    fn new_wraps_verbatim_without_sanitizing() {
        assert_eq!(Handle::new("chapters:intro").as_str(), "chapters:intro");
    }
}

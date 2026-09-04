//! Names rheo has retired: `rheo.toml` keys and authored `#let rheo-*`
//! bindings. Different domains, one file — a retirement is recorded once, so
//! adding a future removal is a single edit, and the build-time warning and
//! `rheo migrate`'s report read the same list.
//!
//! Keys retired from `rheo.toml` in a past version, still parsed harmlessly
//! into an `extra` flatten map (see [`super::Spine::extra`]) but otherwise
//! inert. One shared table so the parse-time warning ([`warn_on_retired_keys`])
//! and `rheo migrate`'s reporting read the same list instead of keeping two
//! in step.

use tracing::warn;

/// A single retired `rheo.toml` key: the table it lived in, its name, and
/// what to do instead.
pub struct RetiredKey {
    /// Table the key lived in, exactly as printed in the warning (e.g. `"[spine]"`).
    pub table: &'static str,
    /// Key name as it appears in `rheo.toml`.
    pub key: &'static str,
    /// What changed, or what to do instead.
    pub replacement: &'static str,
}

/// Every key retired so far.
pub const RETIRED_KEYS: &[RetiredKey] = &[
    RetiredKey {
        table: "[spine]",
        key: "vertebrae",
        replacement: "spine membership and order now come from a directory scan plus \
            `exclude` / `[[section]]` — see https://rheo.ohrg.org/spines",
    },
    RetiredKey {
        table: "[spine]",
        key: "merge",
        replacement: "PDF combines its spine into one document by default and HTML/EPUB \
            always produce per-page output — there is no equivalent key",
    },
    RetiredKey {
        table: "[html]",
        key: "feed_base_url",
        replacement: "Atom feed generation moved to the Typst package @rheo/feeds — \
            see https://rheo.ohrg.org/feeds",
    },
    RetiredKey {
        table: "[html]",
        key: "feed_author",
        replacement: "set via @rheo/feeds instead — see https://rheo.ohrg.org/feeds",
    },
    RetiredKey {
        table: "[html]",
        key: "feed_title",
        replacement: "set via @rheo/feeds instead — see https://rheo.ohrg.org/feeds",
    },
    RetiredKey {
        table: "[html]",
        key: "feed_include",
        replacement: "a marrow that builds its own entry list knows what it put in — this \
            concept does not survive the move to @rheo/feeds",
    },
];

/// A `#let rheo-<name>` binding retired from authored Typst, and what to do
/// instead. The other half of the retirement record: a key lives in a config
/// table, a binding in a `.typ` file, and neither has any effect any more.
pub struct RetiredBinding {
    /// Binding name as it appears after `#let`.
    pub name: &'static str,
    /// What changed, or what to do instead.
    pub replacement: &'static str,
}

/// Every binding retired so far.
pub const RETIRED_BINDINGS: &[RetiredBinding] = &[
    RetiredBinding {
        name: "rheo-feed-title",
        replacement: "moved to @rheo/feeds's Typst configuration",
    },
    RetiredBinding {
        name: "rheo-feed-updated",
        replacement: "moved to @rheo/feeds's Typst configuration",
    },
    RetiredBinding {
        name: "rheo-feed-exclude",
        replacement: "moved to @rheo/feeds's Typst configuration",
    },
    RetiredBinding {
        name: "rheo-author",
        replacement: "use `#set document(author: \"...\")` instead",
    },
];

/// Warn once for each retired key found in `extra`.
///
/// `shown_table` is the table the key was actually authored in, and is what the
/// warning prints; a per-format `[pdf.spine]` matches the `[spine]` entries in
/// [`RETIRED_KEYS`], since one entry has to serve every spine table.
///
/// Call once per authored table instance — once for the global `[spine]`, once
/// for each `[<plugin>.spine]` — never once per build format, or a multi-format
/// project would see the same key warned repeatedly.
pub fn warn_on_retired_keys(shown_table: &str, extra: &toml::Table) {
    let declared_table = if shown_table.ends_with(".spine]") {
        "[spine]"
    } else {
        shown_table
    };
    for retired in RETIRED_KEYS.iter().filter(|r| r.table == declared_table) {
        if extra.contains_key(retired.key) {
            warn!(
                "`{}` in {} is retired and has no effect — {}",
                retired.key, shown_table, retired.replacement
            );
        }
    }
}

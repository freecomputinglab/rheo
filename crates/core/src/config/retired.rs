//! Keys retired from `rheo.toml` in a past version, still parsed harmlessly
//! into an `extra` flatten map (see [`super::Spine::extra`]) but otherwise
//! inert. One shared table so the build-time warning ([`super::validation`])
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

/// Every key retired so far. `rheo-drop-feed-bdo` appends the `[html]` feed
/// keys here once it deletes their fields from `HtmlConfig`.
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
];

/// Warn once for each retired key (matching `table`) found in `extra`.
///
/// Call once per authored table instance — once for the global `[spine]`,
/// once for each `[<plugin>.spine]` — never once per build format, or a
/// multi-format project would see the same key warned repeatedly.
pub fn warn_on_retired_keys(table: &str, extra: &toml::Table) {
    for retired in RETIRED_KEYS.iter().filter(|r| r.table == table) {
        if extra.contains_key(retired.key) {
            warn!(
                "`{}` in {} is retired and has no effect — {}",
                retired.key, retired.table, retired.replacement
            );
        }
    }
}

---
id: rh-auto-index-must-not-refill-an-exclusion-c6b49937
short-id: c6
title: auto_index must not refill an exclusion
priority: 4
labels:
- fix-auto-index-exclusion
deps:
- blocked-by:rh-mark-synthesized-vertebrae-in-spine-flat-607892ff
closed: true
---
Touches: crates/core/src/reticulate/spine/scan.rs

`auto_index` silently overrides an explicit `[spine] exclude`.

Excluding a directory's landing file does not remove that page — rheo
synthesizes a replacement at the very output path the author excluded.

MEASURED, with `content/index.typ`, `content/chapters/index.typ` (containing
`= Real chapters index`) and `content/chapters/one.typ`:

    [spine]
    exclude = ["chapters/index.typ"]

`cargo run -- compile <path> --html` produces `build/html/chapters.html`
containing a generated link list. The author said "do not publish this page";
rheo published a different page at that path.

Cause, `crates/core/src/reticulate/spine/scan.rs:158-173`: `entries` is filtered
through `is_excluded` BEFORE the landing-file search runs, so an excluded
`index.typ` is invisible to the search. `landing_path` comes back `None`, the
directory still has children, and the `None if auto_index` arm at
`crates/core/src/reticulate/spine/scan.rs:210` synthesizes one.

The rule to implement: `auto_index` fills an ABSENCE, never an exclusion. A
directory whose landing file exists but was excluded becomes a group node — the
`auto_index = false` behaviour — because the author has said what they want
there, and it is "nothing".

Steps:

1. `crates/core/src/reticulate/spine/scan.rs:158` — keep the existing filtered
   `entries` exactly as it is; the children loop and everything downstream
   depend on it. Additionally capture the UNFILTERED entry paths from
   `Self::read_sorted_entries(dir)?` before the `.filter(...)` call. A second
   `Vec<PathBuf>` is fine — this directory listing is already in memory.
2. After `landing_path` is computed (line 173), compute a boolean saying whether
   a file named `index.typ` or `<dirname>.typ` exists among the UNFILTERED
   entries. Name it for why it exists, e.g. `landing_excluded`: true when the
   unfiltered search finds one and `landing_path` is `None`.
3. At `crates/core/src/reticulate/spine/scan.rs:210`, guard the synthesis arm:
   `None if auto_index && !landing_excluded =>`. An excluded landing file then
   falls through to the existing group-node arm at line 216.
4. Put a one-line comment on that guard stating the rule: `auto_index` fills an
   absence, not an exclusion.

NON-GOALS:

- Do NOT warn or error on this case. Excluding a landing file and getting a
  group node is a coherent thing to ask for, not a mistake to report.
- Do NOT change what happens when the directory's ONLY `.typ` file was excluded.
  `children.is_empty()` already drops the node entirely at
  `crates/core/src/reticulate/spine/scan.rs:205`, and that arm runs before this
  one. Leave it alone.
- Do NOT touch `apply_sections` or `apply_include` in
  `crates/core/src/reticulate/spine/section.rs`. A separate bird covers the
  different bug where layering moves every child away from a synthesized index
  and leaves an empty page behind.

VERIFY, all four:

1. `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
2. The measured case above now produces NO `build/html/chapters.html`, while
   `build/html/chapters/one.html` still exists.
3. With the same fixture and no `exclude` line at all, `chapters.html` IS
   produced — the default `auto_index` path is unchanged.
4. A unit test in `crates/core/src/reticulate/spine/scan.rs`, beside
   `scan_dir_without_index_synthesizes_landing_when_auto_index_on` (roughly line
   122): a fixture with `extras/index.typ` and `extras/note.typ`, scanned with
   `auto_index = true` and `exclude = ["extras/index.typ"]`, yields an `extras`
   node whose `vertebra()` is `None` and whose `title()` is `Some("Extras")`.
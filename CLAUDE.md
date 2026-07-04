# CLAUDE.md

## Project

**rheo** compiles Typst documents to PDF, HTML, and EPUB. Written in Rust using the Typst compiler as a library.

**Source structure:**
- `crates/cli/` — CLI entry point, `resolve_assets`, compilation orchestration
- `crates/core/` — Config, plugin trait (`FormatPlugin`), `PluginContext`, `AssetConfig`, world, spine, compilation
- `crates/html/` — HTML plugin (dev server, CSS/JS injection)
- `crates/pdf/` — PDF plugin
- `crates/epub/` — EPUB plugin
- `src/typ/rheo.typ` — Core Typst template (auto-injected)
- `build/` — Output dir (gitignored): `pdf/`, `html/`, `epub/`

Integration tests and examples live in [freecomputinglab/rheo-tests](https://github.com/freecomputinglab/rheo-tests), cloned side-by-side at `../rheo-tests` for sibling path dependencies.

## Development Commands

```bash
cargo build
cargo run -- compile <project-path>          # all formats
cargo run -- compile <path> --pdf|--html|--epub
cargo run -- compile <file.typ>              # single file
cargo run -- watch <project-path> --open     # dev server at localhost:3000
cargo run -- clean <project-path>
RUST_LOG=rheo=trace cargo run -- compile ... # debug logging

# Tests
cargo test                                    # run unit tests only
# Integration tests run from ../rheo-tests with:
RHEO_MANIFEST=../rheo/Cargo.toml cargo test --test harness
See [TESTING.md](TESTING.md) for more test commands and options.
cargo fmt && cargo clippy -- -D warnings
```

## rheo.toml

```toml
version = "0.2.1"        # required, must match CLI version
content_dir = "content"  # optional
build_dir = "build"      # optional
formats = ["html", "pdf", "epub"]  # default formats
font_dirs = ["fonts"]    # optional; replaces autoscan of fonts/ directory
copy = ["*.txt"]         # optional; glob patterns copied to every plugin output dir

[html]
feed_base_url = "https://example.com"  # optional; when set, emits build/html/feed.xml (Atom)
feed_author = "Jane Doe"               # optional; atom:author of the feed (default "Rheo")
feed_title = "My Feed"                 # optional; feed <title> and autodiscovery link title (default: spine title → project name)

[html.assets]
copy = ["images/**"]     # optional; glob patterns copied to html output dir only
css_stylesheet = "custom.css"   # optional; path override for AssetConfig name

# Multiple asset blocks: each [[html.assets]] contributes its own
# overrides and copy patterns. All sources are copied verbatim by default.
[[html.assets]]
css_stylesheet = "one.css"
js_scripts     = "one.js"
dest           = "subdir"  # optional; output subdirectory for this block's files

[[html.assets]]
css_stylesheet = "two.css"
js_scripts     = "two.js"

[pdf.spine]
title = "My Book"
vertebrae = ["cover.typ", "chapters/**/*.typ"]

[epub]
identifier = "urn:uuid:..."  # optional, auto-generated
date = 2025-01-15T00:00:00Z

[epub.spine]
title = "My Book"
vertebrae = ["cover.typ", "chapters/**/*.typ"]
```

Precedence: CLI flags > rheo.toml > built-in defaults. Without rheo.toml, title and spine are inferred from filename/directory.

**Font directory resolution:** Without `font_dirs` in config, `fonts/` at project root is auto-discovered. Setting `font_dirs` replaces autoscan (include `"fonts"` explicitly if desired). `--font-dir` CLI flag always appends.

## Cross-file references

rheo assigns each vertebra a canonical label derived from its path relative to the content directory. Link to it with standard Typst anchor syntax:

```typst
#link(<intro>)[Link text]
#link(<chapters:intro>)[nested page]
```

**Root-level files** (`content/intro.typ`) get a bare label: `<intro>`.

**Nested files** use `:` as path separator: `content/chapters/intro.typ` → `<chapters:intro>`. `:` and `.` are valid Typst label characters; `/` is not.

**Escape form:** `<handle.typ>` is always available as an alias (e.g. `<intro.typ>`, `<chapters:intro.typ>`). Useful when the canonical label is taken by a user-authored label.

**Canonical-skip rule:** if a user-authored label in the project already uses the canonical name, rheo silently skips injecting it — the vertebra is still reachable via its escape form.

**Escape-collision error:** if the escape label (`<handle.typ>`) collides with any user-authored label or another vertebra's escape label, the build fails with an error naming the offending file and label.

## Spine configuration

**Spine vertebrae:** The `vertebrae` array in `[pdf.spine]` or `[epub.spine]` specifies which source files to include and in what order. Glob patterns are supported (e.g., `chapters/**/*.typ`).

PDF combines its spine into a single document by default; HTML and EPUB always produce per-page outputs. A `merge` key left in an old `rheo.toml` is silently ignored.

## rheo-* variables and the Atom feed

**Generic variable convention:** any top-level `#let rheo-<key> = <value>` in a vertebra is harvested during compilation. The value must be a string or boolean literal (e.g. `"title"` or `true`) — any other RHS is a compile error. Plugins read these per-file with the `rheo-` prefix stripped (e.g. `rheo-feed-title` is available as `feed-title`).

**Atom feed:** set `feed_base_url` under `[html]` to enable it. When set, the HTML build emits `build/html/feed.xml` (Atom 1.0) with one `<entry>` per vertebra (every vertebra appears by default), and injects a `<link rel="alternate" type="application/atom+xml">` autodiscovery tag into every page's `<head>`. Without `feed_base_url`, no feed is emitted. The feed's `atom:author` defaults to `Rheo`; set `[html] feed_author = "..."` to override it. The feed's `<title>` (and autodiscovery link title) defaults to the HTML spine title, then the project/directory name; set `[html] feed_title = "..."` to override both.

Feed variables (all optional):
- `rheo-feed-title` — override for the entry title; defaults to the `#set document(title: [...])` value.
- `rheo-feed-updated` — override for the entry timestamp (RFC 3339); defaults to the `#set document(date: datetime(...))` value, then the source file's mtime.
- `rheo-feed-exclude` — set to the boolean `true` (`#let rheo-feed-exclude = true`) to omit a vertebra from the feed. Any other value (or absent) includes the page. Useful for cover/index pages.

**Feed content region:** each entry's `<content>` is taken from the first `<main>` element, else the first element with class `rheo-feed-content`, else the whole `<body>`. To exclude page chrome (header/footer/nav) from feed entries, wrap the article in `<main>` (e.g. `html.elem("main", doc)`) and keep the chrome outside it. With no marker, the full body is used.

## Code Style

- `cargo fmt` before committing
- `cargo clippy` — fix all warnings
- Errors via `thiserror`, logging via `tracing` macros
- INFO logs: natural language. DEBUG: implementation details.

## Release

1. Update version in `Cargo.toml`
2. PR title = version tag (e.g. `v0.2.0`) + `release` label
3. Merge triggers automated build, crates.io publish, GitHub Release

---

## Workflow (jj · beads · Plan Mode)

These shared processes live in the computer-wide `~/.claude/CLAUDE.md`. For the churn/pair
"when done" step, this project's formatter/linter is `cargo fmt && cargo clippy -- -D warnings`.

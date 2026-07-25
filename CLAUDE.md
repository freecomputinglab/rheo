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

[spine]
exclude = ["drafts/**"]  # optional; glob patterns (relative to content_dir) omitted from every format's scan

[[spine.section]]
name = "chapters"        # optional; virtual-directory regrouping without moving files on disk
include = ["ch-*.typ"]

[pdf.spine]
title = "My Book"        # per-format override; only the fields set here replace the global [spine]'s

[epub]
identifier = "urn:uuid:..."  # optional, auto-generated
date = 2025-01-15T00:00:00Z

[epub.spine]
title = "My Book"
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

## rheo-context

rheo injects a per-vertebra Typst binding `rheo-context()` into every spine file, exposing rheo's view of the project to authored Typst and to packages. It is a zero-arg **function** prepended to each file's source, returning a dictionary composed of this file's own `handle` plus the format-global values spread from `sys.inputs.rheo-context`: `#let rheo-context() = (handle: <handle>, ..sys.inputs.rheo-context)`. Only the per-file `handle` is baked per vertebra; the shared (potentially large) `spine` is stored once on `sys.inputs`, not duplicated into every file. `sys.inputs` reads need no `#context`, so reading its fields (`rheo-context().handle`) does **not** require the `#context` keyword. The function form also lets an author mock it under vanilla Typst.

Fields (the returned dictionary may gain fields later):
- `handle` — this file's `:`-separated handle (its ID; the same handle used for the cross-file links above). The only per-file field.
- `spine` — the structured spine **tree**, mirroring directory/section nesting. Each node is a dict `(title, handle, path, metadata, children)`: a leaf (a vertebra) carries its own `handle`/`path`/`title`/`metadata`; a group node (a directory or `[[spine.section]]` with no landing file) carries `handle: none`, `path: none`, an empty `metadata: (:)`, and its own group title, nesting its children.
- `spine-flat` — the flat pre-order list of every *clickable* vertebra (groups excluded), each an entry `(handle, path, title, metadata)`.
- `metadata` (per entry) — the named arguments of that vertebra's `#set document(...)` rule, harvested into a dict: e.g. `entry.metadata.at("keywords", default: ())` or `entry.metadata.at("author", default: none)`. Typst's `document` element is not extensible, so the reachable fields are `title`, `author`, `keywords` — all strings (bracket-content is flattened to plain text) or arrays of strings — and `date`, a real Typst `datetime(...)` value when `#set document(date: datetime(...))` is present and resolvable (`entry.metadata.at("date", default: none)`). An entry with no `#set document(...)`, or whose `date` is `none`/`auto`/`datetime.today()`/malformed, has no `date` key (and an entry with no `#set document(...)` at all has an empty `metadata: (:)`).
- `target` — the rheo output-format name (`"epub"`/`"html"`/…). Present only for formats that set one; **absent for PDF**, where documents fall back to Typst's native `target()` == `"paged"`.
- `ext` — the output file extension (`"html"`/`"xhtml"`), gated like `target` (present for html/epub, absent for PDF). The value core reads to build depth-relative cross-vertebra link hrefs.

`spine`/`spine-flat`/`target`/`ext` are format-global (identical across vertebrae), stored once on `sys.inputs.rheo-context`; `rheo-context()` composes them with the per-file `handle`.

```typst
This is #rheo-context().handle of #rheo-context().spine-flat.len() pages.
```

**Passing it to packages:** a package template can't read the file's local `rheo-context()` implicitly — Typst functions capture their definition scope, not the call site — so hand it in explicitly:

```typst
#import "@rheo/somepackage": template
#show: template.with(ctx: rheo-context())
```

**Detecting a rheo build (for a friendly native-Typst error).** rheo also seeds `sys.inputs` with a `rheo-context` key (a plain **data dict**, not a function) carrying the file-independent context (`spine`/`spine-flat`/`target`/`ext`). Because `sys.inputs` is global to the whole bundle compile, it does **not** carry the per-file `handle` — that comes from calling the per-vertebra `rheo-context()`. A package (or author) uses `sys.inputs` to detect a rheo build and turn native-Typst compilation into a friendly message:

```typst
#show: template.with(ctx: if "rheo-context" in sys.inputs { rheo-context() } else {
  panic("This document must be compiled with Rheo — https://rheo.ohrg.org")
})
```

A package needing only the shared spine can read `sys.inputs.rheo-context.spine` directly and never call the per-file `rheo-context()`. (`rheo migrate` rewrites the old bare `rheo-context` binding to the `rheo-context()` call form.)

**Output format.** rheo injects a `target()` polyfill into every file so Typst's own `target()` returns the output format (`"epub"`/`"html"`, or native `"paged"` for PDF), reading it from `sys.inputs.rheo-context.target`. Authored files should detect the format with `target()` (e.g. `target() == "epub"`); it is the only per-file API. The underlying `sys.inputs.rheo-context.target` is the same value for every vertebra but is not in scope where the polyfill isn't (it is global, so reachable via `sys.inputs`). The older `sys.inputs.rheo-target` key has been **removed**.

**Section-label namespacing is a package concern, not core.** rheo does not rewrite or prefix authored labels; label semantics stay exactly as Typst defines them (two vertebrae defining the same label collide as an ordinary Typst duplicate-label error). Packages such as `@rheo/notebox` use `rheo-context()` to *additively* synthesize globally-unique `<handle:label>` section anchors alongside the author's own labels.

## Spine configuration

**Directory-scan default:** with no `[spine]`/`[<format>.spine]` at all, the spine is every `.typ` file under `content_dir`, recursively, ordered alphabetically per directory level. A directory whose landing file is `index.typ` or `<dirname>.typ` gets that directory's own handle (e.g. `chapters/chapters.typ` → `<chapters>`, not `<chapters:chapters>`); a directory with no landing file becomes a non-clickable group node with a prettified title (`01-intro/` → "Intro").

**`[spine] exclude`:** glob patterns (relative to `content_dir`) for files/folders to omit from the scan.

**`[[spine.section]]`:** groups matched files under a virtual directory without moving them on disk (e.g. `include = ["ch-*.typ"]` under `name = "chapters"` gives those files handles like `<chapters:ch-1>`). Nests via `[[spine.section.section]]`.

**Precedence — field-by-field, not whole-table:** a per-format `[<format>.spine]` table can set `title`/`exclude`/`section` independently; any field it leaves unset falls back to the matching field on the global `[spine]` table (not the whole table at once). For example, `[pdf.spine] title = "My Book"` with no `exclude` of its own still inherits the global `[spine] exclude` — it does *not* silently drop it just because `[pdf.spine]` exists.

The retired `vertebrae` glob-list key (pre-0.5.0) is no longer read; `rheo migrate` converts an old inclusion-filter `vertebrae` list into an equivalent `exclude`.

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
- Doc comments on general abstractions (traits, generic types, module docs)
  describe the abstraction, not its implementors. Avoid "the first impl",
  "X is the first", or naming concrete impls (e.g. a trait doc referencing a
  specific type that implements it). Concrete examples belong on the concrete
  type's own docs.
- Prefer a data-structure-first approach over standalone functions: model the
  thing as a type and hang behaviour off it as methods (e.g. `TypstLiteral` with
  a `serialize` method, `LabelSites::from_source`), rather than free functions
  that pass data around. Reach for a struct/enum + `impl` before a bare `fn`.

## Release

1. Update version in `Cargo.toml`
2. PR title = version tag (e.g. `v0.2.0`) + `release` label
3. Merge triggers automated build, crates.io publish, GitHub Release

---

## Workflow (jj · beads · Plan Mode)

These shared processes live in the computer-wide `~/.claude/CLAUDE.md`. For the churn/pair
"when done" step, this project's formatter/linter is `cargo fmt && cargo clippy -- -D warnings`.

# CLAUDE.md

## Project

**rheo** compiles Typst documents to PDF, HTML, and EPUB. Written in Rust using the Typst compiler as a library.

A `FormatPlugin` does format transport — compile, write, serve — and nothing else. Derived artifacts (feeds, sitemaps, search indexes, generated index pages) belong in Typst, minted from a `.marrow.typ`; rheo's job is only to provide the primitives that make them expressible (see "Marrow-authored derived artifacts" below).

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
cargo run -- clean <path> --packages            # also drop cached repo checkouts for [packages] namespaces
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

# Where a package namespace resolves from. Optional and rarely needed: with no
# [packages] table, @rheo resolves from its built-in releases host and every
# other namespace goes to Typst universe.
[packages.rookery]
releases = "freecomputinglab/rookery"  # <owner>/<repo>, or a URL template
                                       # carrying {name} and {version}

[packages.rheo]                        # overrides the built-in @rheo
repo = "https://github.com/freecomputinglab/rheo-packages"  # any URL git accepts
branch = "feat-x"                      # or tag = "...", or rev = "<sha>"
subdir = ""                            # optional path prefix inside the repo

[packages.demo]                        # resolves from a directory on disk
path = "../pkgs"                       # a package's own working tree, read in place
```

**`[packages.<namespace>]`** declares where one namespace comes from. Set exactly one of `repo`, `releases` or `path` — switching a project between a release, a branch and a local directory is an explicit edit, not a precedence rule.

- `releases` takes an `<owner>/<repo>` shorthand (detected by having no scheme), expanded to GitHub's download base, or a full URL template containing both `{name}` and `{version}` for any other forge. Assets are `<name>-<version>.tar.gz` under the tag `<name>-<version>`.
- `repo` takes any URL the `git` binary accepts — https, ssh, or a local path. `branch` (default `main`), `tag` and `rev` select the ref; when more than one is set the precedence is `rev`, then `tag`, then `branch`, and the losing keys are warned about rather than silently dropped. `subdir` (default empty) is a path prefix inside the repository, so `@<ns>/<name>:<version>` lives at `<subdir>/<name>/<version>/`.
- `path` points straight at a directory holding `<name>/<version>/` package trees — no git, no clone, no cache, so an edit to the package shows up on the very next build. A relative `path` resolves against `rheo.toml`'s own directory. There is no `branch`/`tag`/`rev` alongside it (there is no ref to select); `subdir` still applies. **This is machine-local** — it is a line you flip on your own checkout, not something a teammate's build can see, and rheo has no local-override config file to keep it out of a committed `rheo.toml`.

The namespace key must be a Typst identifier, since it appears in every import spec as `@<namespace>/name:1.0.0`.

Precedence: CLI flags > rheo.toml > built-in defaults. Without rheo.toml, title and spine are inferred from filename/directory.

**Font directory resolution:** Without `font_dirs` in config, `fonts/` at project root is auto-discovered. Setting `font_dirs` replaces autoscan (include `"fonts"` explicitly if desired). `--font-dir` CLI flag always appends.

**Publication facts live in Typst, not rheo.toml.** A feed's title, author, and site base URL are deliberately not config keys — a package minting a derived artifact from marrow (e.g. `@rheo/feeds`) takes them as literal Typst values or its own package-level config.

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

rheo injects a per-vertebra Typst binding `rheo-context()` into every spine file, exposing rheo's view of the project to authored Typst and to packages. It is a zero-arg **function** prepended to each file's source, returning a dictionary composed of this file's own `handle`, a `metadata-of` closure, and the format-global values spread from `sys.inputs.rheo-context`: `#let rheo-context() = (handle: <handle>, metadata-of: rheo-metadata, ..sys.inputs.rheo-context)`. Only the per-file `handle` is baked per vertebra; the shared (potentially large) `spine` is stored once on `sys.inputs`, not duplicated into every file. `sys.inputs` reads need no `#context`, so reading its fields (`rheo-context().handle`, `.spine`, `.spine-flat`, `.target`, `.ext`) does **not** require the `#context` keyword. The function form also lets an author mock it under vanilla Typst.

Fields (the returned dictionary may gain fields later):
- `handle` — this file's `:`-separated handle (its ID; the same handle used for the cross-file links above). The only per-file field.
- `spine` — the structured spine **tree**, mirroring directory/section nesting. Each node is a dict `(title, handle, path, children)`: a leaf (a vertebra) carries its own `handle`/`path`/`title`; a group node (a directory or `[[spine.section]]` with no landing file) carries `handle: none`, `path: none`, and its own group title, nesting its children. **`title` here is always path-derived** (the file/directory name, prettified) — it does not reflect any `#set document(title: ...)` a vertebra authors, literal or otherwise (see `docs/limitations.md`).
- `spine-flat` — the flat pre-order list of every *clickable* vertebra (groups excluded), each an entry `(handle, path, title)`. Same path-derived-only caveat as the tree's `title`.
- `metadata-of` — a function value, `(handle) => dict`, that reads another vertebra's resolved `#set document(...)` values live off the compiled bundle (a "metadata beacon" each vertebra publishes after its own body). Returns a dict with whichever of `title`/`author`/`description`/`keywords`/`date` the vertebra actually set (absent keys are omitted, not `none`) — most authoring forms work (literal, via an imported `#show:` template, non-literal expressions, multiple `#set document(...)` rules), but a value set inside a **bounded** `#{ }`/`#[ ]` code block does not (Typst's own block scoping hides it from the beacon's `#context` read, even though the vertebra's own compiled `<title>`/DocumentInfo is unaffected — see `docs/limitations.md`). `title`/`description` come back as real Typst **content**, not flattened strings; `author`/`keywords` are always arrays; `date` is a real `datetime`. Returns `(:)` for a handle with no beacon (combined PDF layouts never emit one). Since it's a dict field rather than a method, call it as `(rheo-context().metadata-of)(handle)`, **not** `rheo-context().metadata-of(handle)`. **Requires `#context`** — it calls `query(...)` internally, unlike every other `rheo-context()` field, which reads straight off `sys.inputs` and needs no `#context` at all. The same underlying helper (`rheo-metadata(handle)`, plus a `rheo-metadata-all()` companion returning one entry per vertebra) is also in scope directly inside a `.marrow.typ` at the bundle root, for building a feed/sitemap/index over every vertebra's metadata at once.
- `target` — the rheo output-format name (`"epub"`/`"html"`/…). Present only for formats that set one; **absent for PDF**, where documents fall back to Typst's native `target()` == `"paged"`.
- `ext` — the output file extension (`"html"`/`"xhtml"`), gated like `target` (present for html/epub, absent for PDF). The value core reads to build depth-relative cross-vertebra link hrefs.

`spine`/`spine-flat`/`target`/`ext` are format-global (identical across vertebrae), stored once on `sys.inputs.rheo-context`; `rheo-context()` composes them with the per-file `handle` and `metadata-of`.

```typst
This is #rheo-context().handle of #rheo-context().spine-flat.len() pages.

// Reading another vertebra's real (not path-derived) title needs #context:
#context [#(rheo-context().metadata-of)("chapters:intro").at("title", default: [Untitled])]
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

A package needing only the shared spine can read `sys.inputs.rheo-context.spine` directly and never call the per-file `rheo-context()`. `rheo-context` is no longer the only key on `sys.inputs`: a project seeds its own via a `rheo.toml` `[inputs]` table and `--input KEY=VALUE` (values always strings, `rheo-context` itself reserved), which is how a build script parameterises a compile — see `docs/contract.md`'s "Project-supplied `sys.inputs`". (`rheo migrate` rewrites the old bare `rheo-context` binding to the `rheo-context()` call form.)

**Output format.** rheo injects a `target()` polyfill into every file so Typst's own `target()` returns the output format (`"epub"`/`"html"`, or native `"paged"` for PDF), reading it from `sys.inputs.rheo-context.target`. Authored files should detect the format with `target()` (e.g. `target() == "epub"`); it is the only per-file API. The underlying `sys.inputs.rheo-context.target` is the same value for every vertebra but is not in scope where the polyfill isn't (it is global, so reachable via `sys.inputs`). The older `sys.inputs.rheo-target` key has been **removed**.

**Section-label namespacing is a package concern, not core.** rheo does not rewrite or prefix authored labels; label semantics stay exactly as Typst defines them (two vertebrae defining the same label collide as an ordinary Typst duplicate-label error). Packages such as `@rheo/notebox` use `rheo-context()` to *additively* synthesize globally-unique `<handle:label>` section anchors alongside the author's own labels.

## Spine configuration

**Directory-scan default:** with no `[spine]`/`[<format>.spine]` at all, the spine is every `.typ` file under `content_dir`, recursively, ordered alphabetically per directory level. A directory whose landing file is `index.typ` or `<dirname>.typ` gets that directory's own handle (e.g. `chapters/chapters.typ` → `<chapters>`, not `<chapters:chapters>`); a directory with no landing file becomes a non-clickable group node with a prettified title (`01-intro/` → "Intro").

**`[spine] exclude`:** glob patterns (relative to `content_dir`) for files/folders to omit from the scan.

**`[[spine.section]]`:** groups matched files under a virtual directory without moving them on disk (e.g. `include = ["ch-*.typ"]` under `name = "chapters"` gives those files handles like `<chapters:ch-1>`). Nests via `[[spine.section.section]]`.

**Precedence — field-by-field, not whole-table:** a per-format `[<format>.spine]` table can set `title`/`exclude`/`section` independently; any field it leaves unset falls back to the matching field on the global `[spine]` table (not the whole table at once). For example, `[pdf.spine] title = "My Book"` with no `exclude` of its own still inherits the global `[spine] exclude` — it does *not* silently drop it just because `[pdf.spine]` exists.

The retired `vertebrae` glob-list key (pre-0.5.0) is no longer read; `rheo migrate` converts an old inclusion-filter `vertebrae` list into an equivalent `exclude`.

PDF combines its spine into a single document by default; HTML and EPUB always produce per-page outputs. A `merge` key left in an old `rheo.toml` is silently ignored.

## Marrow-authored derived artifacts

A `.marrow.typ` at the bundle root (a project's own, or one shipped by a package) mints extra output files from Typst, outside every vertebra's own `#document(...)` block. Three primitives make it possible to build a feed, sitemap, search index, or generated index page there instead of in a plugin crate:

**Transclusion.** A bundle-emitted asset may contain `<rheo-content page="..." select="..." as="escaped|raw"/>`, replaced after compilation with the inner HTML of the selected region of that compiled page. `select` defaults to a cascade: the first `<main>` element, else the first element carrying the `rheo-content` class, else the whole `<body>`; `select` can also name a bare tag or a leading-dot class explicitly. `as` is `escaped` (default, for `<content type="html">`) or `raw` (verbatim, for `<content type="xhtml">`). This exists because marrow runs *inside* the Typst compile, before any page HTML exists, and Typst's `html` module exposes only element/frame construction — no content-to-HTML-string function.

**Head contributions.** A `<rheo-head>` wrapper anywhere in a page's body has its children hoisted into that page's own `<head>`. A `.rheo/head.html` control asset minted from marrow instead appends to *every* page's `<head>`, after each page's own hoisted content. Both exist because Typst builds `<head>` solely from the compiled `DocumentInfo` — there is no other author hook into it.

**Control assets.** The `.rheo/` bundle-output prefix is reserved: an asset minted under it (e.g. `.rheo/head.html`) is a message from the bundle to rheo, consumed during compilation and never written to the actual build output.

`@rheo/feeds` (in `../rheo-packages`) is where Atom feed generation now lives, built on these three primitives plus `rheo-metadata-all()` (see `rheo-context` above) — no Rust code, no plugin, no `rheo.toml` keys.

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
- **Terse and DRY.** Write the smallest code that does the job. Two call sites
  needing the same logic means extracting it, not copying it with a comment
  saying it mirrors the other — if an issue's steps describe logic that already
  exists somewhere ("does X exactly as `foo` does"), factor out the shared part
  instead.
- **Comments earn their place.** A comment states what the code cannot: a
  non-obvious invariant, a required ordering, a deliberate omission that would
  otherwise read as an oversight. It does not restate the line below it,
  narrate the steps, mark sections, cite the line numbers of similar code, or
  explain why the change was made — that belongs in the commit message, the
  bead, or `docs/`. Doc comments on public items stay to a line or two unless
  the abstraction genuinely needs more. Prose pasted from an issue body into
  the source is not documentation.
- Deleting a feature means deleting it. No commented-out code, no tombstone
  comment explaining what used to be there; git history and `rheo migrate`
  carry that.

## Release

1. Update version in `Cargo.toml`
2. PR title = version tag (e.g. `v0.2.0`) + `release` label
3. Merge triggers automated build, crates.io publish, GitHub Release

---

## Workflow (jj · beads · Plan Mode)

These shared processes live in the computer-wide `~/.claude/CLAUDE.md`. For the churn/pair
"when done" step, this project's formatter/linter is `cargo fmt && cargo clippy -- -D warnings`.

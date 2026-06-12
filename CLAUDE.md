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
merge = true

[epub]
identifier = "urn:uuid:..."  # optional, auto-generated
date = 2025-01-15T00:00:00Z

[epub.spine]
title = "My Book"
vertebrae = ["cover.typ", "chapters/**/*.typ"]
```

Precedence: CLI flags > rheo.toml > built-in defaults. Without rheo.toml, title and spine are inferred from filename/directory.

**Font directory resolution:** Without `font_dirs` in config, `fonts/` at project root is auto-discovered. Setting `font_dirs` replaces autoscan (include `"fonts"` explicitly if desired). `--font-dir` CLI flag always appends.

## rheo-* variables and the Atom feed

**Generic variable convention:** any top-level `#let rheo-<key> = "<string>"` in a vertebra is harvested during compilation. The value must be a string literal — a non-string RHS is a compile error. Plugins read these per-file with the `rheo-` prefix stripped (e.g. `rheo-feed-title` is available as `feed-title`).

**Atom feed:** set `feed_base_url` under `[html]` to enable it. When set, the HTML build emits `build/html/feed.xml` (Atom 1.0) with one `<entry>` per vertebra that declares `rheo-feed-title`, and injects a `<link rel="alternate" type="application/atom+xml">` autodiscovery tag into every page's `<head>`. Without `feed_base_url`, no feed is emitted. The feed's `atom:author` defaults to `Rheo`; set `[html] feed_author = "..."` to override it. The feed's `<title>` (and autodiscovery link title) defaults to the HTML spine title, then the project/directory name; set `[html] feed_title = "..."` to override both.

Feed variables:
- `rheo-feed-title` — entry title; **required** for a vertebra to appear in the feed.
- `rheo-feed-updated` — entry timestamp (RFC 3339); optional, falls back to the source file's mtime.

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

## Version Control (jj — NEVER use git)

**NEVER run `jj git push` (or any push) — the user always pushes themselves.**
Prepare commits and bookmarks, then stop and let the user push.

**NEVER run `git` commands, not even read-only ones** (`git log`, `git show`, `git status`, `git diff`). Always use the jj equivalents (`jj log`, `jj show`, `jj status`, `jj diff`, `jj file show`). This applies in sibling repos too (e.g. `../rheo-tests`).

```bash
jj status / jj diff / jj log / jj show
jj commit -m "message" / jj describe -m "message"
jj new / jj new main / jj edit <commit> / jj abandon
jj squash / jj split / jj restore <file>
jj git fetch / jj rebase -d main
```

**PR workflow:**
```bash
jj bookmark create feat/<kebab-case-title> -r @-
# user pushes (e.g. `jj git push --allow-new`)
gh pr create --base main --head feat/<name> --title "..." --body "- bullet\n- bullet"
```

**Commit messages:** Present tense, user-focused. "Displays X in Y", not "Added X" or "Add X".

**PR body:** 3-5 concise bullets. No "This PR", no LLM-style verbosity.

---

## Issue Tracking (beads/br — NEVER use markdown TODOs)

```bash
br ready --json                              # find unblocked work
br list --status=open
br show <id>
br create "Title" -t bug|feature|task -p 0-4 --json
br update <id> --status in_progress --json
br close <id1> <id2> --reason "Done" --json
br dep add <issue> <depends-on>
```

**Priorities:** 0=critical, 1=high, 2=medium, 3=low, 4=backlog

**Local-only:** `.beads/` is gitignored, never commit it, never run `br sync`.

---

## The br/jj Workflow (ALWAYS use for br tasks)

**Session prerequisite** — verify jj identity:
```bash
jj config list --user
# If missing:
jj config set --user user.name "Lachlan Kermode"
jj config set --user user.email "lachie@ohrg.org"
```

**Per-task sequence:**
1. `br update <id> --status in_progress`
2. `jj log` — if empty unnamed commit below working commit, name it: `jj describe -m "..."`
3. `jj new` — fresh working commit
4. Do the work, run tests
5. `jj squash` then `jj describe -r @- -m "Present tense description"`
6. `jj log` — verify history shows correct author on each commit (not empty/unknown)
7. `br close <id> --reason "Done"`

---

## br/jj Churn (only when user says "br/jj churn")

**Before first loop iteration** — verify jj identity (commits without author are broken):
```bash
jj config list --user
# Must show user.name and user.email. If missing:
jj config set --user user.name "Lachlan Kermode"
jj config set --user user.email "lachie@ohrg.org"
```

Loop until no open issues:
1. `br ready --json` — pick highest priority (bugs/tasks/features, not epics/chores)
2. Implement with br/jj workflow
3. `/clear` — clear context
4. Repeat

When done:
```bash
cargo fmt
cargo clippy --fix --all-targets --all-features --allow-dirty -- -D warnings
# jj squash if changes made
```

Report: list all closed issues.

---

## br/jj Pair (only when user says "br/jj pair")

**Before first loop iteration** — verify jj identity (commits without author are broken):
```bash
jj config list --user
# Must show user.name and user.email. If missing:
jj config set --user user.name "Lachlan Kermode"
jj config set --user user.email "lachie@ohrg.org"
```

Loop until no open issues or user stops:
1. `br ready --json` — pick highest priority (bugs/tasks/features, not epics/chores)
2. Implement with br/jj workflow
3. **Pause and prompt the user** — present what was done, ask whether to continue
   - User may review code, request changes, add/modify/remove br issues
   - Only continue to next issue when the user explicitly says to (e.g. "continue", "next", "go")
   - If user says "stop" or "done", exit the loop

When done:
```bash
cargo fmt
cargo clippy --fix --all-targets --all-features --allow-dirty -- -D warnings
# jj squash if changes made
```

Report: list all closed issues.

---

## Plan Mode (activated by "plan mode", "let's plan", "design this", or any prompt ending with "BEADS")

**Rules:** No code, no file edits (except `.beads/`). Output is beads issues only.

**Workflow:**
1. Understand goal, ask clarifying questions
2. Decompose into discrete br issues with type, priority, acceptance criteria
3. Present proposal to user, ask if they want to create the issues
4. If yes: run `br create` commands (parallel where possible), set up deps with `br dep add`
   - Each issue's `--description` must be procedural and unambiguous — written as if for an agent with no prior context. Include: background, relevant file paths and line numbers, exact steps to implement, and the expected outcome. The implementer must not need to investigate or infer anything.
5. List created IDs and stop — do NOT implement, do NOT ask if user wants to implement

**Exits** when user says "br/jj churn", "br/jj pair", "start implementing", or "go".

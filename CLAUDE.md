# CLAUDE.md

## Project

**rheo** compiles Typst documents to PDF, HTML, and EPUB. Written in Rust using the Typst compiler as a library.

**Source structure:**
- `src/rs/` — Rust: `main.rs`, `cli.rs`, `compile.rs`, `world.rs`, `project.rs`, `output.rs`, `assets.rs`
- `src/typ/rheo.typ` — Core Typst template (auto-injected)
- `build/` — Output dir (gitignored): `pdf/`, `html/`, `epub/`

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
cargo test                                    # run all tests
See [TESTING.md](TESTING.md) for more test commands and options.
cargo fmt && cargo clippy -- -D warnings
```

## rheo.toml

```toml
version = "0.2.0"        # required, must match CLI version
content_dir = "content"  # optional
build_dir = "build"      # optional
formats = ["html", "pdf", "epub"]  # default formats
font_dirs = ["fonts"]    # optional; replaces autoscan of fonts/ directory
copy = ["*.txt"]         # optional; glob patterns copied to every plugin output dir

[html.assets]
copy = ["images/**"]     # optional; glob patterns copied to html output dir only
css_stylesheet = "custom.css"   # optional; path override for AssetConfig name

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

```bash
jj status / jj diff / jj log / jj show
jj commit -m "message" / jj describe -m "message"
jj new / jj new main / jj edit <commit> / jj abandon
jj squash / jj split / jj restore <file>
jj git fetch / jj rebase -d main
jj git push -c @- / jj git push --allow-new
```

**PR workflow:**
```bash
jj bookmark create feat/<kebab-case-title> -r @-
jj git push --allow-new
gh pr create --base main --head feat/<name> --title "..." --body "- bullet\n- bullet"
```

**Commit messages:** Present tense, user-focused. "Displays X in Y", not "Added X" or "Add X".

**PR body:** 3-5 concise bullets. No "This PR", no LLM-style verbosity.

---

## Issue Tracking (beads/bd — NEVER use markdown TODOs)

```bash
bd ready --json                              # find unblocked work
bd list --status=open
bd show <id>
bd create "Title" -t bug|feature|task -p 0-4 --json
bd update <id> --status in_progress --json
bd close <id1> <id2> --reason "Done" --json
bd dep add <issue> <depends-on>
```

**Priorities:** 0=critical, 1=high, 2=medium, 3=low, 4=backlog

**Local-only:** `.beads/` is gitignored, never commit it, never run `bd sync`.

---

## The bd/jj Workflow (ALWAYS use for bd tasks)

**Session prerequisite** — verify jj identity:
```bash
jj config list --user
# If missing:
jj config set --user user.name "Lachlan Kermode"
jj config set --user user.email "lachie@ohrg.org"
```

**Per-task sequence:**
1. `bd update <id> --status in_progress`
2. `jj log` — if empty unnamed commit below working commit, name it: `jj describe -m "..."`
3. `jj new` — fresh working commit
4. Do the work, run tests
5. `jj squash` then `jj describe -r @- -m "Present tense description"`
6. `jj log` — verify history shows correct author on each commit (not empty/unknown)
7. `bd close <id> --reason "Done"`

---

## bd/jj Churn (only when user says "bd/jj churn")

**Before first loop iteration** — verify jj identity (commits without author are broken):
```bash
jj config list --user
# Must show user.name and user.email. If missing:
jj config set --user user.name "Lachlan Kermode"
jj config set --user user.email "lachie@ohrg.org"
```

Loop until no open issues:
1. `bd ready --json` — pick highest priority (bugs/tasks/features, not epics/chores)
2. Implement with bd/jj workflow
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

## Plan Mode (activated by "plan mode", "let's plan", "design this", or any prompt ending with "BEADS")

**Rules:** No code, no file edits (except `.beads/`). Output is beads issues only.

**Workflow:**
1. Understand goal, ask clarifying questions
2. Decompose into discrete bd issues with type, priority, acceptance criteria
3. Present proposal to user, ask if they want to create the issues
4. If yes: run `bd create` commands (parallel where possible), set up deps with `bd dep add`
   - Each issue's `--description` must be procedural and unambiguous — written as if for an agent with no prior context. Include: background, relevant file paths and line numbers, exact steps to implement, and the expected outcome. The implementer must not need to investigate or infer anything.
5. List created IDs and stop — do NOT implement, do NOT ask if user wants to implement

**Exits** when user says "bd/jj churn", "start implementing", or "go".

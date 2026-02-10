# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## Project-Specific Configuration

### Project Description

**rheo** is a tool for flowing Typst documents into publishable outputs. It compiles Typst files to multiple output formats including PDF, HTML, and EPUB.

**Architecture:**
- Written in Rust using the Typst compiler as a library
- CLI tool built with clap for command-line argument parsing
- Implements custom `World` trait for Typst compilation with automatic `rheo.typ` import injection
- Uses typst-kit for font discovery and management

**Key Features:**
- Multi-format compilation (PDF, HTML, and EPUB)
- Project-based compilation (compiles all .typ files in a directory)
- **Incremental compilation in watch mode** using Typst's comemo caching
- Automatic asset copying (CSS, images) for HTML output
- Clean command for removing build artifacts
- Template injection for consistent document formatting
- Configurable default output formats via rheo.toml
- **Smart defaults for EPUB** (automatic title and spine inference)

**Project Structure:**
- `src/rs/` - Rust source code
  - `main.rs` - CLI entry point
  - `lib.rs` - Library root
  - `cli.rs` - Command-line interface and argument parsing
  - `compile.rs` - PDF and HTML compilation logic
  - `world.rs` - Typst World implementation for file access
  - `project.rs` - Project detection and configuration
  - `output.rs` - Output directory management
  - `assets.rs` - Asset copying utilities
  - `logging.rs` - Logging configuration
  - `error.rs` - Error types
- `src/typ/` - Typst template files
  - `rheo.typ` - Core template and utilities

Each project creates its own `build/` directory (gitignored) containing:
- `pdf/` - PDF outputs
- `html/` - HTML outputs
- `epub/` - EPUB outputs

### Development Commands

**Build the project:**
```bash
cargo build
```

**Run rheo:**
```bash
# Compile a project directory
cargo run -- compile <project-path>
cargo run -- compile <project-path> --pdf    # PDF only
cargo run -- compile <project-path> --html   # HTML only
cargo run -- compile <project-path> --epub   # EPUB only

# Compile a single .typ file
cargo run -- compile <file.typ>              # All formats
cargo run -- compile <file.typ> --pdf        # PDF only
cargo run -- compile <file.typ> --html       # HTML only
cargo run -- compile <file.typ> --epub       # EPUB only

# Examples
cargo run -- compile examples/blog_site                      # Directory mode
cargo run -- compile examples/blog_site/content/index.typ    # Single file mode
cargo run -- compile examples/blog_post --epub               # EPUB with defaults

# Using custom config location
cargo run -- compile examples/blog_site --config /path/to/custom.toml

# Using custom build directory
cargo run -- compile examples/blog_site --build-dir /tmp/build
```

**Additional CLI flags:**
```bash
# --config: Load rheo.toml from custom location (overrides default ./rheo.toml)
cargo run -- compile <project-path> --config /path/to/config.toml
cargo run -- watch <project-path> --config /path/to/config.toml

# --build-dir: Override build directory (takes precedence over rheo.toml setting)
cargo run -- compile <project-path> --build-dir /tmp/rheo-build
cargo run -- watch <project-path> --build-dir ./custom-output

# Both flags work with compile, watch, and clean commands
cargo run -- clean <project-path> --build-dir /tmp/rheo-build
```

**Clean build artifacts:**
```bash
cargo run -- clean                            # Clean current directory's project
cargo run -- clean <project-path>             # Clean specific project
cargo run -- clean examples/blog_site         # Example: clean blog_site project
```

**Run with debug logging:**
```bash
RUST_LOG=rheo=trace cargo run -- compile <project-path>
```

**Run tests:**
```bash
# Run all tests
cargo test

# Run integration tests only
cargo test --test harness

# Update test references (after intentional output changes)
UPDATE_REFERENCES=1 cargo test --test harness

# Run only HTML tests (across all projects that support it)
RUN_HTML_TESTS=1 cargo test --test harness

# Run only PDF tests (across all projects that support it)
RUN_PDF_TESTS=1 cargo test --test harness

# Run only EPUB tests (across all projects that support it)
RUN_EPUB_TESTS=1 cargo test --test harness

# Increase diff output limit (default: 2000 chars)
RHEO_TEST_DIFF_LIMIT=10000 cargo test --test harness -- --nocapture

# Run tests sequentially (to avoid parallel conflicts)
cargo test --test harness -- --test-threads=1
```

**Note:** Tests automatically use embedded fonts (`TYPST_IGNORE_SYSTEM_FONTS=1`) for consistent output across environments. This is passed to all subprocess invocations by the test harness.

See `tests/README.md` for detailed documentation on the integration test suite.

**Test Suite Features:**
- **Directory Tests**: Full project compilation with rheo.toml
- **Single-File Tests**: Individual .typ files with test markers
- **Test Markers**: Embedded comments in .typ files declaring test metadata
- **Format Filtering**: Environment variables to run only HTML or PDF tests
- **Improved Error Messages**: Detailed diffs with statistics and update commands
- **Hash-Based References**: Prevents conflicts between single-file tests

### Configuration (rheo.toml)

Projects can include a `rheo.toml` configuration file in the project root to customize compilation behavior.

**Example rheo.toml:**
```toml
version = "0.1.2"

content_dir = "content"

[compile]
# Default formats to compile when no CLI flags are specified
# Default: ["pdf", "html", "epub"]
formats = ["html", "pdf"]

```

**Configuration Precedence:**
- CLI flags (`--pdf`, `--html`, `--epub`) override config file formats
- If no CLI flags are specified, uses `compile.formats` from config
- If `compile.formats` is empty or not specified, defaults to `["html", "epub", "pdf"]`

### Complete Configuration Reference

**Full rheo.toml schema with all available options:**

```toml
# Manifest version (required)
version = "0.1.2"  # Required: Manifest version for rheo.toml API compatibility
                   # Must be valid semver (e.g., "0.1.2")
                   # Current supported version: 0.1.2

# Project-level configuration
content_dir = "content"  # Directory containing .typ files (relative to project root)
                         # If not specified, searches entire project root
                         # Example: "content", "src", "chapters"

build_dir = "build"      # Build output directory (relative to project root unless absolute)
                         # Defaults to "build/" if not specified
                         # Examples: "output", "../shared-build", "/tmp/rheo-build"

formats = ["html", "pdf", "epub"]  # Default formats to compile when no CLI flags specified
                                    # Defaults to all three formats if not specified
                                    # Valid values: "html", "pdf", "epub"

# HTML-specific configuration
[html]
stylesheets = ["style.css"]  # CSS files to inject into HTML output
                             # Paths are relative to build/html directory
                             # Default: ["style.css"]

fonts = []                   # External font URLs to inject into HTML
                             # Example: ["https://fonts.googleapis.com/css2?family=Inter"]
                             # Default: []

# PDF-specific configuration
[pdf]
# Optional: Configure PDF spine for multi-chapter books
[pdf.spine]
title = "My Book"           # Title for the PDF document
vertebrae = ["cover.typ", "chapters/**/*.typ"]  # Glob patterns for files to include
                                                # Patterns evaluated relative to content_dir
                                                # Results sorted lexicographically
                                                # Example patterns:
                                                #   - "cover.typ" (single file)
                                                #   - "chapters/**" (all files in chapters/)
                                                #   - "**/*.typ" (all .typ files recursively)
merge = true                # Optional: merge vertebrae into single PDF (default: false)

# EPUB-specific configuration
[epub]
identifier = "urn:uuid:12345678-1234-1234-1234-123456789012"  # Unique global identifier
                                                               # Optional, auto-generated if not specified
                                                               # Format: URN, URL, or ISBN

date = 2025-01-15T00:00:00Z  # Publication date (ISO 8601 format)
                              # Optional, separate from modification timestamp
                              # Default: current date if not specified

# Optional: Configure EPUB spine for multi-chapter books
[epub.spine]
title = "My Book"           # Title for the EPUB document
vertebrae = ["cover.typ", "chapters/**/*.typ"]  # Glob patterns for files to include
                                                # Same format as pdf.spine.vertebrae
```

**Configuration Field Details:**

**Top-level fields:**
- `version` (string, required): Manifest version for rheo.toml API compatibility. Must be valid semver (e.g., "0.1.2"). The manifest version must match the rheo CLI version. Current supported version: 0.1.2
- `content_dir` (string, optional): Directory containing .typ source files. If omitted, searches entire project root.
- `build_dir` (string, optional): Output directory for compiled files. Defaults to `./build`.
- `formats` (array of strings, optional): Default output formats. Defaults to `["html", "epub", "pdf"]`.

**[html] section:**
- `stylesheets` (array of strings): CSS files to inject. Paths relative to `build/html/`. Default: `["style.css"]`.
- `fonts` (array of strings): External font URLs to inject into HTML `<head>`. Default: empty.
- `spine` (object, optional): Configuration for HTML output (multiple files, not merged).
  - `title` (string, required if spine used): Title for the HTML site.
  - `vertebrae` (array of strings, required if spine used): Glob patterns for files to include.
  - `merge` (boolean, optional): Ignored for HTML (always produces per-file output). Defaults to None.

**[pdf] section:**
- `spine` (object, optional): Configuration for merging multiple .typ files into a single PDF.
  - `title` (string, required if spine used): Title for the merged PDF.
  - `vertebrae` (array of strings, required if spine used): Glob patterns for files to include, sorted lexicographically.
  - `merge` (boolean, optional): Whether to merge files into single PDF. Defaults to false.

**[epub] section:**
- `identifier` (string, optional): Unique identifier for the EPUB (URN, URL, or ISBN). Auto-generated if omitted.
- `date` (datetime, optional): Publication date in ISO 8601 format. Defaults to current date.
- `spine` (object, optional): Configuration for merging multiple .typ files into a single EPUB.
  - `title` (string, required if spine used): Title for the merged EPUB.
  - `vertebrae` (array of strings, required if spine used): Glob patterns for files to include, sorted lexicographically.
  - `merge` (boolean, optional): Ignored for EPUB (always merges). Defaults to None.

**Precedence rules:**
1. CLI flags (`--pdf`, `--html`, `--epub`, `--config`, `--build-dir`) take highest precedence
2. rheo.toml settings apply if no CLI flags specified
3. Built-in defaults apply if field not specified in rheo.toml

### Manifest Versioning

rheo.toml files must include a version field that matches the rheo CLI version.

- **Required field**: Every rheo.toml must have `version = "0.1.2"` (quoted string)
- **Semantic versioning**: Uses full semver format (major.minor.patch)
- **Exact match required**: rheo warns if config version doesn't match CLI version
- **Current version**: 0.1.2
- **When to bump**: Manifest version now tracks the CLI version (bumped with each release)

**Error handling:**
- **Missing version**: Error at config load time with message to add version field
- **Invalid version**: Error with explanation of expected semver format (must be quoted string like "0.1.2")
- **Version mismatch**: Warning (non-fatal) suggesting rheo.toml version update

### Default Behavior Without rheo.toml

When no `rheo.toml` exists, rheo automatically infers sensible defaults for EPUB compilation:

**Title Inference:**
- **Single-file mode**: Derived from filename (e.g., `my-document.typ` → "My Document")
- **Directory mode**: Derived from folder name (e.g., `my_book` → "My Book")

**EPUB Spine Inference:**
- **Single-file mode**: Just the single file
- **Directory mode**: All `.typ` files sorted lexicographically (equivalent to `**/*.typ` pattern)

**Format Behavior:**
- **HTML**: Works per-file (one HTML file per `.typ` file)
- **PDF**: Works per-file by default (merge requires explicit config)
- **EPUB**: Always merged (uses inferred title and spine)

**Example - Single file without config:**
```bash
# Compile a single file to EPUB without any config
cargo run -- compile document.typ --epub
# Generates document.epub with title "Document"
```

**Example - Directory without config:**
```bash
# Compile a directory to EPUB without any config
cargo run -- compile my_project/ --epub
# Generates my_project.epub with:
#   - Title: "My Project"
#   - Spine: All .typ files in lexicographic order
```

**Note:** Existing projects with explicit `rheo.toml` configurations are not affected—explicit configs always take precedence over inferred defaults.

### Format Detection in Typst Code

Rheo polyfills the `target()` function for EPUB compilation, so you can use standard Typst patterns:

**Basic usage (recommended):**

```typst
// target() returns "epub" for EPUB, "html" for HTML, "paged" for PDF
#context if target() == "epub" {
  [EPUB-specific content]
} else if target() == "html" {
  [HTML-specific content]
} else {
  [PDF content]
}
```

**Helper Functions (available via rheo.typ injection):**

```typst
// Explicit helpers for format checking
#if is-rheo-epub() { [EPUB-only content] }
#if is-rheo-html() { [HTML-only content] }
#if is-rheo-pdf() { [PDF-only content] }
```

**How it works:**
- Rheo sets `sys.inputs.rheo-target` to "epub", "html", or "pdf"
- For EPUB compilation, a `target()` polyfill is injected that checks `sys.inputs.rheo-target`
- This shadows the built-in `target()` so `target() == "epub"` works naturally
- The polyfill is syntactic sugar for user code convenience

**For Typst library/package authors:**

The `target()` polyfill only shadows the local function name. Packages that call `std.target()` (common practice to get the "real" target) will bypass the polyfill and see "html" for EPUB compilation.

To properly support rheo's EPUB detection, library authors should check `sys.inputs.rheo-target` directly:

```typst
// Recommended pattern for libraries
#let get-format() = {
  if "rheo-target" in sys.inputs {
    sys.inputs.rheo-target  // "epub", "html", or "pdf" when compiled with rheo
  } else {
    target()  // Fallback for vanilla Typst
  }
}
```

This pattern:
- Returns the correct format when compiled with rheo
- Gracefully degrades to standard `target()` in vanilla Typst
- Works regardless of whether the package calls `target()` or `std.target()`

### Incremental Compilation

**Overview:**
Rheo's watch mode uses incremental compilation to achieve 3x-100x faster recompilation speeds compared to cold compilation. This is powered by Typst's comemo (constrained memoization) system.

**How It Works:**
1. **World Reuse**: A single `RheoWorld` instance is created at watch startup and reused across all recompilations
2. **Cache Reset**: Before each recompilation, `world.reset()` clears file caches while preserving fonts, library, and packages
3. **Main File Switching**: `world.set_main()` updates which file is being compiled without recreating the World
4. **Comemo Caching**: Typst's memoization system caches compilation results and only recomputes changed parts
5. **Memory Management**: `comemo::evict(10)` after each compilation prevents unbounded cache growth

**Architecture:**
- `compile_pdf()` / `compile_html()` - Regular functions for single compilation (compile command)
- `compile_pdf_incremental()` / `compile_html_incremental()` - Optimized functions that accept existing World (watch mode)
- `perform_compilation()` - Used by compile command (creates fresh World per file)
- `perform_compilation_incremental()` - Used by watch mode (reuses World across files)

**Testing Incremental Compilation:**
```bash
# Start watch mode - directory
cargo run -- watch examples/blog_site --html

# Start watch mode - single file
cargo run -- watch examples/blog_site/content/index.typ --html

# In another terminal, make a small edit to a file
echo "\n// Test change" >> examples/blog_site/content/index.typ

# Observe recompilation time in watch output
# Initial compilation: ~2-3 seconds for all files
# Incremental recompilation: ~100-500ms for changed file
```

**Performance Characteristics:**
- **Cold compilation** (first run or after config change): Full compilation of all files
- **Incremental compilation** (file edit in watch mode): Only recompiles changed files with cached dependencies
- **Memory usage**: Stabilizes due to `comemo::evict(10)` after each compilation
- **Cache invalidation**: Automatic based on file content changes

**Key Implementation Files:**
- `src/rs/world.rs` - `RheoWorld::reset()` and `RheoWorld::set_main()` methods
- `src/rs/compile.rs` - `compile_*_incremental()` functions
- `src/rs/cli.rs` - Watch loop with World creation and reuse (lines 631-692)
- `Cargo.toml` - `comemo = "0.5"` dependency for cache management

### Development Server and Live Reload

**Overview:**
Rheo includes a built-in development server with automatic browser refresh for HTML output. The server is activated with the `--open` flag in watch mode, providing a seamless development experience.

**How It Works:**
1. **Server Activation**: Use `--open` flag with watch command to start the server
2. **HTTP Server**: Runs on `http://localhost:3000` (port hardcoded, not configurable)
3. **SSE Endpoint**: Server-Sent Events endpoint at `/events` for browser communication
4. **Live Reload Script**: HTML files automatically include a script that connects to the SSE endpoint
5. **File Change Detection**: When Typst files change and recompile, server broadcasts reload events
6. **Browser Auto-Refresh**: Connected browsers receive the reload event and refresh automatically

**Architecture:**
- `src/rs/server.rs` - Development server implementation using axum
- Port 3000 is hardcoded (see `cli.rs` line 598)
- SSE-based communication for zero-configuration live reload
- Serves static HTML files from the build/html directory
- Broadcast channel pattern for one-to-many client notifications

**Usage:**
```bash
# Start watch mode with development server
cargo run -- watch examples/blog_site --open

# Server starts at http://localhost:3000
# Browser opens automatically showing index.html
# Edit any .typ file - browser refreshes automatically
```

**Key Features:**
- Zero-configuration setup
- Automatic browser opening
- Instant refresh on file changes
- Works with incremental compilation for fast iteration
- Supports multiple connected browsers simultaneously

**Implementation Details:**
- Server state includes broadcast channel and HTML directory path
- Static file handler serves .html files from build directory
- SSE handler streams reload events to connected clients
- Server runs in background task, doesn't block watch loop

### Error Formatting and Logging

**Overview:**
Rheo uses codespan-reporting for rich, user-friendly error and warning messages. Errors from Typst compilation are displayed with source context, line numbers, and color highlighting.

**Error Output Format:**
- **Colored output**: Automatically enabled when stderr is a TTY
- **Source context**: Shows relevant code lines with error markers
- **Line numbers**: Displays using `│` box-drawing characters
- **Multi-error aggregation**: All errors reported before failing

**Logging Levels:**
- **Normal mode** (default): Shows user-friendly INFO-level messages
  - Project loading, compilation progress, success/failure
  - Example: ` INFO compiling to PDF input=portable_epubs.typ`
- **Verbose mode** (`-v`): Shows DEBUG-level implementation details
  - Build directory resolution, config loading, asset copying
  - Example: ` DEBUG build directory dir=./build`
- **Quiet mode** (`-q`): Only shows errors

**Log Message Guidelines:**
- INFO logs use natural language, not technical jargon
- Avoid timestamps (removed for cleaner output)
- No function names in user-facing logs (use spans for debugging)
- Implementation details go to DEBUG level

**Example Error Output:**
```
error: cannot add integer and string
   ┌─ type_error.typ:10:15
   │
10 │ let result = x + y
   │               ^^^^^
```

**Example Warning Output:**
```
warning: block may not occur inside of a paragraph and was ignored
   ┌─ portable_epubs.typ:21:7
   │
21 │       block(body)
   │       ^^^^^^^^^^^
```

**Testing Error Formatting:**
```bash
# Run error formatting tests
cargo test test_error_formatting -- --nocapture

# Run warning formatting tests
cargo test test_warning_formatting -- --nocapture
```

**Implementation Files:**
- `src/rs/formats/common.rs` - `print_diagnostics()` using codespan-reporting
- `src/rs/world.rs` - `Files` trait implementation for RheoWorld
- `src/rs/logging.rs` - Logging configuration (no timestamps, clean output)
- `tests/cases/error_formatting/` - Test files with intentional errors

### Project-Specific Conventions

**Commit Messages:**
- Follow the jj commit message guidelines (present tense, user-focused)
- Examples: "Compiles Typst to PDF and HTML", "Injects rheo.typ automatically"

**Code Style:**
- Use `cargo fmt` before committing
- Fix all clippy warnings: `cargo clippy`
- Errors use thiserror for consistent error handling
- Logging uses tracing macros (info!, warn!, error!)

**Dependencies:**
- Typst libraries are pulled from git main branch
- Keep dependencies minimal and well-justified

### Branching and Release Workflow

**Development Model:**
- All development happens via pull requests to `main`
- The `main` branch is the primary development branch
- No long-lived feature branches; PRs are merged directly to main

**Release Process:**

When ready to cut a new release:

1. **Update version in Cargo.toml** to the new version number

2. **Create a release PR:**
   - PR title MUST be the version tag (e.g., `v0.2.0`)
   - Add the `release` label to the PR

3. **Pre-release validation** (automated via `.github/workflows/pre-release.yml`):
   - Builds and tests on all supported platforms (Linux x86_64/ARM, macOS x86_64/ARM, Windows x86_64/ARM)
   - Validates PR title matches `vX.Y.Z` format
   - Runs `cargo publish --dry-run` to verify crates.io readiness

4. **Merge the PR** - triggers release automation (`.github/workflows/release.yml`):
   - Builds release binaries for all platforms
   - Publishes to crates.io
   - Creates a git tag matching the PR title
   - Creates a GitHub Release with platform-specific zip files
   - Auto-generates release notes from merged PR titles since the last release (no manual changelog needed)

**Supported Platforms:**
- `x86_64-unknown-linux-gnu` (Linux x86_64)
- `aarch64-unknown-linux-gnu` (Linux ARM64)
- `x86_64-apple-darwin` (macOS Intel)
- `aarch64-apple-darwin` (macOS Apple Silicon)
- `x86_64-pc-windows-msvc` (Windows x86_64)
- `aarch64-pc-windows-msvc` (Windows ARM64)

**Release Artifacts:**
Each release includes zip files for each platform containing the `rheo` binary, available on the GitHub Releases page. Zip files are named `rheo-{target}.zip` for compatibility with `cargo binstall rheo`.

---

## Version Control with Jujutsu

**IMPORTANT**: This project uses jj (Jujutsu) exclusively. NEVER use git commands.

### Basic jj Commands
- `jj status` - Show current changes and working copy status
- `jj commit -m "message"` - Commit current changes with a message
- `jj describe -m "message"` - Set/update description of current change
- `jj log` - View commit history (graphical view)
- `jj diff` - Show diff of current changes
- `jj show` - Show details of current commit

### Branch Management
- `jj new` - Create new change (equivalent to git checkout -b)
- `jj new main` - Create new change based on main
- `jj edit <commit>` - Switch to editing a specific commit
- `jj abandon` - Abandon current change

### Synchronization and Pull Requests
- `jj git fetch` - Fetch changes from remote repository
- `jj rebase -d main` - Rebase current change onto main
- `jj git push -c @-` - Push current change and create bookmark (@- refers to the parent of the current change, as the current change is generally empty)

**Pull Request Workflow:**

When you're ready to create a PR from your completed work:

1. **Create bookmark from commit message** - Automatically derive bookmark name from `@-` commit:
   ```bash
   # Example: "Supports single-file compilation" → feat/supports-single-file-compilation
   jj bookmark create feat/<kebab-case-message> -r @-
   ```

2. **Push to GitHub**:
   ```bash
   jj git push --allow-new
   ```

3. **Create PR with gh CLI**:
   ```bash
   gh pr create --base main --head feat/<bookmark-name> \
     --title "<commit-message>" \
     --body "- Bullet point 1
   - Bullet point 2
   - Bullet point 3"
   ```

**Bookmark naming:**
- Prefix: `feat/` for features, `fix/` for bug fixes
- Name: Convert commit message to kebab-case (lowercase, spaces → hyphens)
- Example: "Fixes compilation error" → `fix/fixes-compilation-error`

**PR message format:**
- Title: Use commit message as-is (present tense)
- Body: 3-5 concise bullet points summarizing changes
- Each bullet: verb + what changed (Adds, Updates, Fixes, Implements, etc.)

**After review:**
- `jj git fetch && jj rebase -d main` if needed

**Pull Request Message Guidelines:**
- Keep descriptions concise and technical, avoid LLM-style verbosity
- Focus on what was changed, not implementation details
- Use bullet points for multiple changes
- Avoid phrases like "This PR", "I have implemented", or overly formal language
- Example: "Add while loops to parser and codegen" rather than "This pull request implements comprehensive while loop support across the compiler pipeline"

### Commit Message Guidelines
- Write in imperative mood and present tense
- Be descriptive about what the change accomplishes
- Examples: "Add while loop support to parser", "Fix segfault in code generation", "Add assignment operators to the language"

### Advanced Operations
- `jj split` - Split current change into multiple commits
- `jj squash` - Squash changes into parent commit
- `jj duplicate` - Create duplicate of current change
- `jj restore <file>` - Restore file from parent commit

---

## Issue Tracking with Beads

**IMPORTANT**: This project uses **bd (beads)** for ALL issue tracking. Do NOT use markdown TODOs, task lists, or other tracking methods.

### Why bd?

- Dependency-aware: Track blockers and relationships between issues
- Agent-optimized: JSON output, ready work detection, discovered-from links
- Prevents duplicate tracking systems and confusion
- Local-only: Issues are stored locally, not shared via version control

### Quick Start

**Check for ready work:**
```bash
bd ready --json
```

**Create new issues:**
```bash
bd create "Issue title" -t bug|feature|task -p 0-4 --json
bd create "Issue title" -p 1 --deps discovered-from:bd-123 --json
```

**Claim and update:**
```bash
bd update bd-42 --status in_progress --json
bd update bd-42 --priority 1 --json
```

**Complete work:**
```bash
bd close bd-42 --reason "Completed" --json
```

### Issue Types

- `bug` - Something broken
- `feature` - New functionality
- `task` - Work item (tests, docs, refactoring)
- `epic` - Large feature with subtasks
- `chore` - Maintenance (dependencies, tooling)

### Priorities

- `0` - Critical (security, data loss, broken builds)
- `1` - High (major features, important bugs)
- `2` - Medium (default, nice-to-have)
- `3` - Low (polish, optimization)
- `4` - Backlog (future ideas)

### Workflow for AI Agents

1. **Check ready work**: `bd ready` shows unblocked issues
2. **Claim your task**: `bd update <id> --status in_progress`
3. **Work on it**: Implement, test, document
4. **Discover new work?** Create linked issue:
   - `bd create "Found bug" -p 1 --deps discovered-from:<parent-id>`
5. **Complete**: `bd close <id> --reason "Done"`

### Local-Only Configuration

**IMPORTANT**: This project uses beads as a **local implementation detail only**. The `.beads/` directory is gitignored and NOT shared via version control.

Configuration (`.beads/config.yaml`):
- `no-auto-flush: true` - Disables automatic JSONL export (since not tracked in git)
- `no-auto-import: true` - Disables automatic JSONL import (since not tracked in git)
- Issues are stored in the local SQLite database only

This means:
- ✅ Use bd for local task tracking and workflow management
- ✅ Issues are private to your local checkout
- ❌ Do NOT commit `.beads/` files to version control
- ❌ Issues are NOT shared between team members or machines

### MCP Server (Recommended)

If using Claude or MCP-compatible clients, install the beads MCP server:

```bash
pip install beads-mcp
```

Add to MCP config (e.g., `~/.config/claude/config.json`):
```json
{
  "beads": {
    "command": "beads-mcp",
    "args": []
  }
}
```

Then use `mcp__beads__*` functions instead of CLI commands.

### Important Rules

- ✅ Use bd for ALL task tracking
- ✅ Always use `--json` flag for programmatic use
- ✅ Link discovered work with `discovered-from` dependencies
- ✅ Check `bd ready` before asking "what should I work on?"
- ❌ Do NOT create markdown TODO lists
- ❌ Do NOT use external issue trackers
- ❌ Do NOT duplicate tracking systems

---

## The bd/jj workflow 

**IMPORTANT**: ALWAYS use the jj squash workflow when working on bd tasks, even if you're only implementing a single task. This workflow should be your default approach.

When working through bd (beads) tasks, use the jj squash workflow. This creates a clean commit history where related work is grouped together.

### The Squash Pattern

The workflow maintains two commits:
- **Named commit** (bottom): Empty at first, receives work via squash. Has a descriptive message.
- **Working commit** (top): Unnamed and empty. All changes happen here, then get squashed down.

After squashing, the working commit becomes empty again, and the pattern repeats.

### Per-Task Workflow

For each bd task, follow this sequence:

1. **Name the commit**: Run `jj describe -m "Present tense description"`
   - Message describes what the app does after this change
   - Completes the phrase: "when this commit is applied, the app..."
   - Examples:
     - "Renders timeline using real-world data"
     - "Improves coloration of navbar"
     - "Adds date-based scroll mapping to timeline"
   - Use present tense, NOT past tense or imperative mood
   - Focus on user-visible changes, not implementation details

2. **Create working commit**: Run `jj new`
   - This creates a new empty commit on top where you'll do the work
   - All file changes will go into this commit

3. **Complete the bd task**:
   - Implement the changes
   - Test that it works
   - Close the issue: `bd update <id> --status closed`

4. **Squash the work**: Run `jj squash`
   - Moves all changes from the working commit (top) into the named commit (below)
   - Working commit becomes empty again, ready for next task

5. **Repeat**: Go to step 1 for the next task

### Commit Message Examples

✅ Good (present tense, user-focused):
- "Displays flight hours in timeline visualization"
- "Renders year markers in timeline sidebar"
- "Synchronizes timeline scroll with table position"
- "Shows data gaps as empty bars in timeline"

❌ Bad (wrong tense or too technical):
- "Added TimelineBar component" (past tense)
- "Add timeline visualization" (imperative, not present)
- "Refactors VerticalTimeline.jsx to use new components" (implementation detail)
- "Created data utilities module" (past tense, not user-visible)

### When to Use This Workflow

**ALWAYS use this workflow** when working on bd tasks. This is the standard approach for this project.

The workflow works for:
- Single bd tasks (one task = one commit)
- Multiple related bd tasks (multiple tasks = one commit)
- Any feature or bug fix tracked in bd

Only skip this workflow when:
- User explicitly requests a different approach
- Working on unrelated changes that must be separate commits

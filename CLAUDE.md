# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## Project-Specific Configuration

### Project Description

**rheo** is a tool for flowing Typst documents into publishable outputs. It compiles Typst files to multiple output formats including PDF, HTML, and (planned) EPUB.

**Architecture:**
- Written in Rust using the Typst compiler as a library
- CLI tool built with clap for command-line argument parsing
- Implements custom `World` trait for Typst compilation with automatic `rheo.typ` import injection
- Uses typst-kit for font discovery and management

**Key Features:**
- Multi-format compilation (PDF and HTML currently supported)
- Project-based compilation (compiles all .typ files in a directory)
- **Incremental compilation in watch mode** using Typst's comemo caching
- Automatic asset copying (CSS, images) for HTML output
- Clean command for removing build artifacts
- Template injection for consistent document formatting
- Configurable default output formats via rheo.toml

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
- `src/typst/` - Typst template files
  - `rheo.typ` - Core template and utilities

Each project creates its own `build/` directory (gitignored) containing:
- `pdf/` - PDF outputs
- `html/` - HTML outputs
- `epub/` - EPUB outputs (planned)

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

# Compile a single .typ file
cargo run -- compile <file.typ>              # All formats
cargo run -- compile <file.typ> --pdf        # PDF only
cargo run -- compile <file.typ> --html       # HTML only

# Examples
cargo run -- compile examples/blog_site                      # Directory mode
cargo run -- compile examples/blog_site/content/index.typ    # Single file mode
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
cargo test
```

### Configuration (rheo.toml)

Projects can include a `rheo.toml` configuration file in the project root to customize compilation behavior.

**Example rheo.toml:**
```toml
content_dir = "content"

[compile]
# Default formats to compile when no CLI flags are specified
# Options: "pdf", "html", "epub"
# Default: ["pdf", "html"]
formats = ["html", "pdf"]

# Global exclude patterns - apply to ALL output formats
# These are combined with format-specific excludes
exclude = ["lib/**/*.typ", "**/*.bib"]

[html]
# Unified exclude/include patterns
# - Regular patterns (e.g., "*.tmp") exclude matching files
# - Negated patterns (e.g., "!**/*.typ") include ONLY matching files
# Include only .typ files and images for HTML output:
exclude = ["!**/*.typ", "!img/**", "!css/**"]

[pdf]
# Files to exclude from PDF compilation only (in addition to global excludes)
exclude = ["web/**/*.typ", "index.typ"]

[epub]
# Files to exclude from EPUB compilation only (in addition to global excludes)
exclude = []
```

**HTML Exclude Pattern Syntax:**

The `[html] exclude` field supports both exclude and include-only patterns:

- **Negated patterns** (`!pattern`): Include-only filters. File must match at least one.
  - Example: `!**/*.typ` includes only .typ files
  - Example: `!img/**` includes only files in img/ directory

- **Regular patterns** (`pattern`): Exclude filters. File must not match any.
  - Example: `*.tmp` excludes .tmp files
  - Example: `_drafts/**` excludes _drafts/ directory

- **Combined logic**: File included if not excluded AND (no negations OR matches a negation)

**Pattern Examples:**

Include only .typ files and images:
```toml
[html]
exclude = ["!**/*.typ", "!img/**"]
```

Exclude temps and drafts:
```toml
[html]
exclude = ["*.tmp", "_drafts/**"]
```

Include .typ and images, but exclude temps:
```toml
[html]
exclude = ["!**/*.typ", "!img/**", "*.tmp"]
```

```toml
[html]
exclude = ["!**/*.typ", "!img/**", "!css/**", "index.typ"]
```

Explanation:
- `!**/*.typ` - Include all .typ files for compilation
- `!img/**` - Include all images for copying
- `!css/**` - Include all CSS for copying
- `index.typ` - Exclude index.typ from compilation (regular pattern)

**Global Exclude Patterns:**

The `[compile] exclude` field specifies patterns that apply to **ALL** output formats (HTML, PDF, and EPUB). These global patterns are combined with format-specific excludes to create the complete exclusion set for each format.

**How it works:**
- A file is excluded from a format if it matches EITHER:
  1. A global `compile.exclude` pattern, OR
  2. A format-specific exclude pattern (e.g., `html.exclude`, `pdf.exclude`)

**Example - Global exclusion:**
```toml
[compile]
exclude = ["**/*.bib", "lib/**/*.typ"]  # Excluded from ALL formats
```

**Example - Global + format-specific:**
```toml
[compile]
exclude = ["**/*.bib"]  # Excluded from ALL formats

[html]
exclude = ["img/**"]    # Additionally excluded from HTML only

[pdf]
exclude = ["index.typ"] # Additionally excluded from PDF only
```

For HTML output in this example:
- `**/*.bib` files excluded (global)
- `img/**` files excluded (HTML-specific)
- `index.typ` NOT excluded (that's PDF-specific)

**Example - DRY configuration:**

If you previously duplicated patterns across formats:
```toml
# Old (duplicated)
[html]
exclude = ["**/*.bib", "index.typ"]

[pdf]
exclude = ["**/*.bib", "web/**/*.typ"]
```

You can simplify using global patterns:
```toml
# New (DRY)
[compile]
exclude = ["**/*.bib"]  # Common exclusion

[html]
exclude = ["index.typ"]

[pdf]
exclude = ["web/**/*.typ"]
```

**Global patterns with negations:**

Global excludes work with include-only patterns too:
```toml
[compile]
exclude = ["**/*.bib"]  # Globally excluded

[html]
exclude = ["!**/*.typ", "!img/**"]  # Include only .typ and images
```

Result: `.bib` files are excluded even though HTML uses include-only mode (global takes precedence).

**Configuration Precedence:**
- CLI flags (`--pdf`, `--html`, `--epub`) override config file formats
- If no CLI flags are specified, uses `compile.formats` from config
- If `compile.formats` is empty or not specified, defaults to `["pdf", "html"]`

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

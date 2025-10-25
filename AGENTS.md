# AGENTS.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

## Project-Specific Configuration

**Instructions:** This section should contain project-specific commands, workflows, and conventions. Customize this section for each repository while keeping the generic tooling sections below unchanged.

### Development Commands

<!-- Add project-specific build, test, and development server commands here -->

### Project-Specific Conventions

<!-- Add any project-specific commit message guidelines, file structure notes, or other conventions here -->

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
1. `jj new main` - Create new change from main
2. Make your changes and test them
3. `jj commit -m "descriptive message"` - Commit changes
4. `jj git push -c @` - Push to remote and create bookmark (note the bookmark name from output)
5. `gh pr create --head <bookmark-name-from-step-4>` - Create pull request using the bookmark name shown in previous step
6. After review: `jj git fetch && jj rebase -d main` if needed

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
- Git-friendly: Auto-syncs to JSONL for version control
- Agent-optimized: JSON output, ready work detection, discovered-from links
- Prevents duplicate tracking systems and confusion

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

### Auto-Sync

bd automatically syncs with git:
- Exports to `.beads/issues.jsonl` after changes (5s debounce)
- Imports from JSONL when newer (e.g., after `git pull`)
- No manual export/import needed!

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

## Combined Workflow: bd Tasks with jj Squash

When working through multiple bd (beads) tasks that should be combined into a single commit, use the jj squash workflow. This creates a clean commit history where related work is grouped together.

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

Use this workflow when:
- Working through a set of related bd tasks
- The tasks together implement one logical feature or change
- You want a single clean commit rather than many small commits

Don't use for:
- Unrelated changes that should be separate commits
- When user explicitly wants granular commit history

# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

**Note**: This project uses [bd (beads)](https://github.com/steveyegge/beads) for issue tracking. Use `bd` commands instead of markdown TODOs. See AGENTS.md for workflow details.

## Project Overview

**rheo** is a generalized tool for flowing Typst documents into publishable outputs. It provides a streamlined build system for converting Typst markup into multiple output formats including PDF, HTML, and EPUB.

## Development Environment

The project uses Nix flakes to manage dependencies. Enter the development environment with:

```bash
nix develop
```

This provides:
- `typst` - Markup language compiler (built from latest main branch)
- `pandoc` - Document converter
- `just` - Command runner
- `fish` - Shell (auto-launched in dev environment)

Update Typst to the latest upstream version:
```bash
just update
```

## Build Commands

All build commands use `just` (a modern alternative to Make):

```bash
# Build all .typ files in the src folder (default project)
just

# Build a specific project folder
just build examples/academic_book
just build examples/blog_site
just build examples/phd_thesis

# Advanced: Compile a single file to both PDF and HTML
just convert path/to/file.typ [PROJECT_NAME]

# Advanced: Compile to PDF or HTML only
just convert-pdf path/to/file.typ [PROJECT_NAME]
just convert-html path/to/file.typ [PROJECT_NAME]

# Clean build artifacts (preserves .gitignore)
just clean
```

## Project Structure

### Source Organization

Projects are organized in folders, each containing:
- `*.typ` - Typst source documents
- `style.css` (optional) - CSS for HTML output (falls back to root style.css if not present)
- `img/` (optional) - Image assets specific to this project
- `references.bib` (optional) - BibTeX bibliography

Example projects:
- `src/` - Default project (built with `just`)
- `examples/academic_book/` - Academic book chapters
- `examples/blog_site/` - Blog posts with images
- `examples/phd_thesis/` - PhD thesis content

Shared resources:
- `bookutils.typ` - Shared Typst template utilities (imported from project files)
- `style.csl` - Citation Style Language definition
- `style.css` - Root-level CSS (fallback for projects without their own)

### Build Output

The build system creates project-specific output directories:

```
build/
├── {project}/
│   ├── pdf/
│   │   └── *.pdf
│   └── html/
│       ├── *.html
│       ├── style.css (copied from project or root)
│       └── img/ (copied from project if exists)
└── .gitignore
```

Examples:
- `just` → outputs to `build/pdf/` and `build/html/`
- `just build examples/blog_site` → outputs to `build/blog_site/pdf/` and `build/blog_site/html/`

### Key Technical Details

**Typst HTML Feature**: The project uses an experimental HTML compilation feature (`--features html`) that's under active development in Typst upstream. The flake tracks the main branch to access this functionality.

**Asset Management**: The build system automatically copies project-specific assets to the output:
- `style.css` is copied from the project folder, or falls back to the root `style.css` if not present
- `img/` directories are copied to the HTML output directory if they exist in the project folder
- This allows each project to have its own styling and images

**Shell Scripts**: Justfile recipes are written in Fish shell syntax. Each multi-line recipe begins with `#!/usr/bin/env fish`.

**File Naming**: Documents may use numbered prefixes (0, 1, 2, etc.) to maintain order. The Justfile strips the `.typ` extension when determining output filenames.

**Project Naming**: When building a folder with `just build FOLDER`, the project name (used for the output directory) is derived from the folder's basename. For example, `just build examples/blog_site` creates output in `build/blog_site/`.

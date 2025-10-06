# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
# Compile all .typ files to both PDF and HTML
just

# Compile a single chapter to PDF and HTML
just convert 0.introduction.typ

# Compile to PDF only
just convert-pdf 0.introduction.typ

# Compile to HTML only
just convert-html 0.introduction.typ

# Clean build artifacts (preserves .gitignore)
just clean
```

## Project Structure

### Source Files

- `*.typ` - Typst source documents
- `references.bib` - BibTeX bibliography
- `style.csl` - Citation Style Language definition
- `style.css` - CSS for HTML output
- `img/` - Image assets
- `examples/` - Example Typst documents

### Build Output

The build system creates:
- `build/pdf/*.pdf` - Compiled PDF chapters
- `build/html/*.html` - Compiled HTML chapters (with style.css copied)
- `build/typst/*.typ` - Intermediate Typst files

### Key Technical Details

**Typst HTML Feature**: The project uses an experimental HTML compilation feature (`--features html`) that's under active development in Typst upstream. The flake tracks the main branch to access this functionality.

**Shell Scripts**: Justfile recipes are written in Fish shell syntax. Each multi-line recipe begins with `#!/usr/bin/env fish`.

**File Naming**: Documents may use numbered prefixes (0, 1, 2, etc.) to maintain order. The Justfile strips the `.typ` extension when determining output filenames.

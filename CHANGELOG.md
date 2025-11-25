# Changelog

All notable changes to rheo will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

---

## [Unreleased]

### Changed

- **BREAKING**: All path patterns in `rheo.toml` are now relative to `content_dir` instead of project root
  - Affects: `static_files`, `compile.exclude`, and format-specific exclude patterns
  - Migration: Remove `content/` prefix from exclude patterns if `content_dir = "content"`
  - See [MIGRATION.md](MIGRATION.md) for detailed migration guide
  - Commit: 23de3861

### Fixed

- Path pattern resolution is now consistent across all configuration options
  - Previously `static_files` used `content_dir` while excludes used project root
  - All patterns now use the same base directory for easier configuration

---

## Previous Changes

Project history before this changelog:
- Multi-format compilation (PDF, HTML)
- Incremental compilation with watch mode
- Template injection system
- Asset management for static files

# Migration Guide

This document describes breaking changes and how to update your configuration.

---

## Path Resolution Changes (commit: 23de3861)

### What Changed

All path patterns in `rheo.toml` are now consistently resolved **relative to `content_dir`**, not the project root.

This affects:
- `static_files` patterns
- `compile.exclude` global patterns
- `[html].exclude` and `[pdf].exclude` format-specific patterns

### Why This Changed

Previously, path resolution was inconsistent:
- `static_files` patterns were relative to `content_dir` ✓
- Global `compile.exclude` patterns were relative to project root ✗
- Format-specific excludes were relative to project root ✗

This inconsistency caused confusion and made patterns harder to write. Now all patterns use the same base directory.

### How to Migrate

If your `rheo.toml` has `content_dir = "content"` and uses exclude patterns, you need to remove the `content/` prefix from your patterns.

**Before:**
```toml
content_dir = "content"

[html]
static_files = ["img/**"]      # Already relative to content_dir

[pdf]
exclude = ["content/index.typ"]  # ✗ Relative to project root
```

**After:**
```toml
content_dir = "content"

[html]
static_files = ["img/**"]      # Still relative to content_dir

[pdf]
exclude = ["index.typ"]        # ✓ Now relative to content_dir
```

### Examples

Given this directory structure:
```
my-project/
├── rheo.toml
└── content/
    ├── index.typ
    ├── chapter1.typ
    └── img/
        └── logo.png
```

And this configuration:
```toml
content_dir = "content"

[html]
static_files = ["img/**"]

[pdf]
exclude = ["index.typ"]
```

**Pattern Resolution:**
- `static_files = ["img/**"]` matches `content/img/**`
- `exclude = ["index.typ"]` matches `content/index.typ`

All patterns start from `content_dir`, making them easier to understand and maintain.

### Checking Your Configuration

After updating your patterns:

1. **Test PDF compilation:**
   ```bash
   cargo run -- compile . --pdf
   ```
   Verify excluded files are not compiled.

2. **Test HTML compilation:**
   ```bash
   cargo run -- compile . --html
   ```
   Verify static files are copied and excludes work correctly.

3. **Run tests:**
   ```bash
   cargo test
   ```

### Need Help?

If you encounter issues migrating your configuration, please open an issue on GitHub with:
- Your directory structure
- Your `rheo.toml` configuration
- The error message or unexpected behavior

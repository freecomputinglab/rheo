# Spike: Typst Bundle Rust API

**Issue:** rheo-l32
**Date:** 2026-03-11
**Status:** Complete

## Goal

Determine how to invoke bundle compilation from the typst Rust crates, and clarify the design boundary between Typst's native bundle API and Rheo's wrapper layer.

---

## Findings

### 1. The `typst-bundle` crate

Bundle compilation lives in a **separate crate** (`typst-bundle`) that is not a dependency of the `typst` crate. It must be added explicitly.

Current status: `typst-bundle` is **not** in rheo's `Cargo.toml`. To add it:

```toml
# In [workspace.dependencies]:
typst-bundle = "0.14.2"

# In [patch.crates-io]:
typst-bundle = { git = "https://github.com/typst/typst", branch = "main" }
```

### 2. Compilation API

Bundle compilation uses the same generic `typst::compile::<T>()` entry point as other formats:

```rust
use typst_bundle::Bundle;
use typst::compile;

// Feature::Bundle (and Feature::Html for HTML docs within the bundle) must be enabled.
let features: Features = [Feature::Html, Feature::Bundle].into_iter().collect();
let library = Library::builder().with_features(features).build();

// Compile to Bundle:
let Warned { output, warnings } = typst::compile::<Bundle>(&world);
let bundle: Bundle = output?;
```

Without `Feature::Bundle`, compilation errors with:
> "bundle export is only available when `--features bundle` is passed"

Rheo's `world.rs` (line 82) currently only enables `Feature::Html` — this must be extended for bundle compilation.

### 3. Bundle Output Types

```rust
pub struct Bundle {
    pub files: Arc<IndexMap<VirtualPath, BundleFile>>,
    pub introspector: Arc<BundleIntrospector>,
}

pub enum BundleFile {
    Document(BundleDocument),   // from #document() element
    Asset(Bytes),                // from #asset() element
}

pub enum BundleDocument {
    Paged(Box<PagedDocument>, PagedExtras),  // pdf, png, svg
    Html(Box<HtmlDocument>),                  // html
}

pub struct PagedExtras {
    pub format: PagedFormat,   // Pdf | Png | Svg
    pub anchors: Vec<(Location, EcoString)>,  // named destinations for cross-links
}
```

### 4. Export API

```rust
use typst_bundle::{BundleOptions, export, VirtualFs};

let options = BundleOptions {
    pixel_per_pt: 144.0,
    pdf: PdfOptions::default(),
};

// Returns IndexMap<VirtualPath, Bytes> — raw bytes keyed by output path
let fs: VirtualFs = typst_bundle::export(&bundle, &options)?;

// Write to disk:
for (path, bytes) in &fs {
    let out_path = output_dir.join(path.get_without_slash());
    fs::create_dir_all(out_path.parent().unwrap())?;
    fs::write(&out_path, bytes)?;
}
```

### 5. Typst Bundle Syntax

A "bundle entry" `.typ` file uses top-level `#document()` and `#asset()` calls:

```typ
// page1.typ — bundle entry file
#document("index.html", title: [Home])[
  = Home
  View #link(<list>)[my famous list].
]

#document("list.html", title: [My Famous List])[
  = My Famous List <list>
  - Item 1
  - Item 2
]

// Copy a file from the project into the bundle output.
#asset("styles.css", read("styles.css"))
```

**Format is inferred from the file extension** of the path argument to `#document()`:
- `.html` → HTML document
- `.pdf` → PDF document
- `.svg` → SVG (single-page only)
- `.png` → PNG (single-page only)

Explicit format override: `#document("out.pdf", format: pdf)[...]`

### 6. Static Pre-compilation AST Analysis

`typst-syntax` (already in workspace deps) can statically detect whether a file is a bundle entry before compilation:

```rust
use typst_syntax::{parse, SyntaxKind};
use typst_syntax::ast::{AstNode, Expr, FuncCall};

fn is_bundle_entry(source: &str) -> bool {
    let root = parse(source);
    for node in root.children() {
        if node.kind() == SyntaxKind::FuncCall {
            if let Some(call) = node.cast::<FuncCall>() {
                if let Expr::Ident(ident) = call.callee() {
                    match ident.get() {
                        "document" | "asset" => return true,
                        _ => {}
                    }
                }
            }
        }
    }
    false
}
```

This enables "TracedSpine" — pre-compilation spine discovery without running the compiler.

### 7. Self-Bundling vs. Rheo-Wrapper Distinction

**Self-bundling file** (detected by top-level `#document()`/`#asset()` calls):
- Pass directly to `typst::compile::<Bundle>()` as-is
- Do NOT inject `rheo_template`, do NOT wrap in Rheo's own `#document()` call
- The file declares its own output structure

**Plain file** (normal Typst content):
- Rheo generates a synthetic bundle entry that wraps the file:
  ```typ
  #document("output.html")[
    #include "/path/to/file.typ"
  ]
  ```
- Or Rheo compiles directly to a single-format output (existing behaviour)

### 8. Introspection Architecture

`BundleIntrospector` spans all documents in the bundle — labels defined in one document are resolvable from another. It wraps per-document introspectors (`PagedIntrospector` or `HtmlIntrospector`) and provides a unified query interface.

The compilation loop runs up to 5 iterations for introspection convergence, same as single-document compilation.

---

## Summary

| Aspect | Detail |
|--------|--------|
| Crate | `typst-bundle` — add to workspace deps + patch |
| Compile call | `typst::compile::<Bundle>(&world)` |
| Feature flags | `Feature::Html + Feature::Bundle` both required |
| Export | `typst_bundle::export(&bundle, &options)` → `IndexMap<VirtualPath, Bytes>` |
| Static detection | `typst_syntax::parse()` + walk AST for `#document`/`#asset` FuncCall |
| Wrapper distinction | Self-bundling = pass through; plain = Rheo wraps |

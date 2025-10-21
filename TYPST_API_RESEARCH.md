# Typst Library API Research

Research for rheo-19: Understanding typst library HTML and PDF compilation API

## Overview

Typst compilation follows a 4-stage pipeline:
1. **Parsing**: Source → Tokens → Syntax Tree → AST
2. **Evaluation**: AST → Module (scope + content tree)
3. **Layouting**: Content → PagedDocument (frames with fixed positions)
4. **Exporting**: Frames → Output format (PDF, HTML, PNG, SVG)

## Key Dependencies Needed

Add to `Cargo.toml`:
```toml
[dependencies]
typst = { git = "https://github.com/typst/typst", branch = "main" }
typst-pdf = { git = "https://github.com/typst/typst", branch = "main" }
typst-html = { git = "https://github.com/typst/typst", branch = "main" }
```

## Core API Components

### 1. The World Trait

The `World` trait defines the compilation environment. It must provide:
- `main()` - Returns the main FileId to compile
- `source(FileId)` - Returns source code for a file
- `library()` - Returns the Typst standard library
- File system access for imports (bookutils.typ, etc.)
- Font access
- Today's date, etc.

**Implementation Strategy:**
We need to create a custom `RheoWorld` struct that implements `World`.
It should:
- Set the root directory to repository root (for finding `src/typst/bookutils.typ`)
- Provide file access for .typ files and imports
- Load fonts from the system

### 2. PDF Compilation

**Function**: `typst_pdf::pdf(document: &PagedDocument, options: &PdfOptions) -> SourceResult<Vec<u8>>`

**Example Usage**:
```rust
use typst::compile;
use typst_pdf::{pdf, PdfOptions};

// Compile to PagedDocument
let result = compile::<PagedDocument>(&world);
let document = match result.output {
    Ok(doc) => doc,
    Err(errors) => return Err(errors),
};

// Export to PDF bytes
let pdf_bytes = pdf(&document, &PdfOptions::default())?;
std::fs::write("output.pdf", pdf_bytes)?;
```

**PdfOptions Fields**:
- `ident`: Smart<&str> - Stable document identifier (use Smart::Auto for us)
- `timestamp`: Option<Timestamp> - Creation timestamp
- `page_ranges`: Option<PageRanges> - Which pages to export (None = all)
- `standards`: PdfStandards - PDF standard conformance
- `tagged`: bool - Whether to create tagged PDF (default: true)

**For rheo**: Use `PdfOptions::default()` which sets reasonable defaults.

### 3. HTML Compilation

**Functions**:
1. `typst::compile::<HtmlDocument>(&world)` - Compiles to HtmlDocument
2. `typst_html::html(&document) -> SourceResult<String>` - Converts HtmlDocument to HTML string

**Example Usage**:
```rust
use typst::compile;
use typst_html::{HtmlDocument, html};

// Compile to HtmlDocument
let result = compile::<HtmlDocument>(&world);
let document = match result.output {
    Ok(doc) => doc,
    Err(errors) => return Err(errors),
};

// Export to HTML string
let html_string = html(&document)?;
std::fs::write("output.html", html_string)?;
```

**Key Differences from PDF**:
- Use `HtmlDocument` instead of `PagedDocument` as the type parameter
- HTML export produces a string, not bytes
- No options struct needed (HTML export is simpler)

### 4. Document Trait and Target

The `compile()` function is generic over types implementing the `Document` trait:
- `PagedDocument` - For PDF, PNG, SVG output
- `HtmlDocument` - For HTML output

The `Target` enum determines which document type to use:
- `Target::Pdf` → `PagedDocument`
- `Target::Html` → `HtmlDocument`

The target must be set in the styles before compilation:
```rust
use typst::foundations::{Target, TargetElem};

let target = TargetElem::target.set(Target::Html).wrap();
let styles = base.chain(&target);
```

However, when using `compile::<HtmlDocument>()`, the type parameter handles this automatically.

### 5. Compilation Result

`compile()` returns `Warned<SourceResult<D>>` where:
- `Warned` wraps both output and warnings
- `SourceResult<D>` is `Result<D, EcoVec<SourceDiagnostic>>`
- Access the document via: `result.output?`
- Access warnings via: `result.warnings`

**Error Handling**:
```rust
let result = compile::<PagedDocument>(&world);

// Print warnings
for warning in &result.warnings {
    eprintln!("Warning: {}", warning);
}

// Get document or errors
let document = match result.output {
    Ok(doc) => doc,
    Err(errors) => {
        for error in errors {
            eprintln!("Error: {}", error);
        }
        return Err(anyhow::anyhow!("Compilation failed"));
    }
};
```

## Implementation Plan for rheo

### Step 1: Create RheoWorld struct (in project.rs or new world.rs)

```rust
use typst::World;
use typst::foundations::Bytes;
use std::path::{Path, PathBuf};

struct RheoWorld {
    root: PathBuf,           // Repository root
    main_file: PathBuf,      // File to compile
    library: prehashed::Library,
    // ... other fields for file cache, fonts, etc.
}

impl World for RheoWorld {
    fn main(&self) -> FileId { /* ... */ }
    fn source(&self, id: FileId) -> FileResult<Source> { /* ... */ }
    fn file(&self, id: FileId) -> FileResult<Bytes> { /* ... */ }
    fn library(&self) -> &Library { &self.library }
    // ... implement other required methods
}
```

**Key Implementation Details**:
- `root` should be set to repository root (`.`)
- This allows Typst to resolve imports like `../../src/typst/bookutils.typ`
- Need to handle file ID mapping (FileId ↔ PathBuf)
- Can look at `typst-cli` or `typst-as-lib` for World implementation examples

### Step 2: Update compile.rs

```rust
pub fn compile_pdf(input: &Path, output: &Path, root: &Path) -> Result<()> {
    // Create World
    let world = RheoWorld::new(root, input)?;

    // Compile to PagedDocument
    let result = typst::compile::<PagedDocument>(&world);

    // Handle warnings
    for warning in &result.warnings {
        eprintln!("Warning: {}", warning);
    }

    // Get document
    let document = result.output?;

    // Export to PDF
    let pdf_bytes = typst_pdf::pdf(&document, &PdfOptions::default())?;

    // Write to file
    std::fs::write(output, pdf_bytes)?;
    Ok(())
}

pub fn compile_html(input: &Path, output: &Path, root: &Path) -> Result<()> {
    // Create World
    let world = RheoWorld::new(root, input)?;

    // Compile to HtmlDocument
    let result = typst::compile::<HtmlDocument>(&world);

    // Handle warnings
    for warning in &result.warnings {
        eprintln!("Warning: {}", warning);
    }

    // Get document
    let document = result.output?;

    // Export to HTML
    let html_string = typst_html::html(&document)?;

    // Write to file
    std::fs::write(output, html_string)?;
    Ok(())
}
```

### Step 3: Handle --root flag equivalent

The Justfile uses `typst compile --root .` to set the repository root.
In our World implementation, we pass the root directory when creating RheoWorld:

```rust
let root = std::env::current_dir()?; // Repository root
let world = RheoWorld::new(&root, input_file)?;
```

This allows imports like `#import "../../src/typst/bookutils.typ"` to resolve correctly.

### Step 4: Handle --features html equivalent

The Justfile uses `--features html` flag. This is likely a build feature for the typst CLI.
For the library usage, we don't need this - the compile function handles everything.

## Common Pitfalls

1. **FileId Mapping**: Must maintain consistent FileId ↔ Path mapping
2. **Library Loading**: Need to provide standard library via `world.library()`
3. **Font Access**: World must provide font access (can use system fonts)
4. **Import Resolution**: Root directory must be set correctly for imports to work
5. **Error Display**: SourceDiagnostic errors should be displayed with proper formatting

## Next Steps

1. Implement RheoWorld struct (look at typst-cli for reference)
2. Add typst-pdf and typst-html dependencies
3. Implement compile_pdf() and compile_html() functions
4. Test with examples to verify imports work
5. Handle error messages properly for user feedback

## References

- [Typst docs.rs](https://docs.rs/typst)
- [typst-as-library example](https://github.com/tfachmann/typst-as-library)
- [typst-as-lib crate](https://github.com/Relacibo/typst-as-lib)
- Typst source: `~/.cargo/git/checkouts/typst-*/crates/`

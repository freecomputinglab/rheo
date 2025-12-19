# Rheo Codebase Refactoring - Beads Epic Design

## Overview

This plan designs a beads epic to refactor the rheo codebase, eliminating brittle patterns and introducing idiomatic Rust abstractions. The refactoring focuses on trait-based design, reducing code duplication, and improving error handling.

## Key Findings Summary

### Code Quality Issues Found
- **400+ `.unwrap()` calls** in production code that could panic
- **200+ lines of duplicated logic** between `perform_compilation()` and `perform_compilation_incremental()`
- **Hardcoded string literals** (".typ", ".html", ".pdf") in 20+ locations
- **Duplicate regex patterns** across multiple files
- **Manual result aggregation** repeated for each format
- **400-500 lines of format handling duplication** that could benefit from traits

### Architectural Opportunities
- **FormatCompiler trait**: Unify PDF/HTML/EPUB compilation (200+ line reduction)
- **Builder pattern**: Improve RheoCompileOptions ergonomics
- **Result aggregation**: Type-safe compilation result tracking
- **Constant extraction**: Centralized file extensions and regex patterns

## Epic Structure

The epic will be organized into **4 phases** with **15 tasks total**:

### Phase 1: Foundation & Safety (4 tasks)
Critical improvements to error handling and constants

### Phase 2: Core Abstractions (5 tasks)
Trait-based design for format compilation

### Phase 3: Deduplication (4 tasks)
Remove duplicated logic in CLI and compilation

### Phase 4: Polish (2 tasks)
Ergonomic improvements and cleanup

---

## Epic and Task Definitions

### Epic: `bd-refactor` (Parent Epic)

**Title:** "Refactor codebase for idiomatic Rust and reduced brittleness"

**Type:** epic

**Priority:** 1 (high)

**Description:**
```
Comprehensive refactoring to improve code quality, reduce duplication, and introduce trait-based abstractions.

Goals:
- Replace production .unwrap() calls with proper error handling
- Introduce FormatCompiler trait to eliminate 400-500 lines of duplication
- Extract constants for file extensions and regex patterns
- Deduplicate perform_compilation functions in cli.rs
- Add builder pattern for RheoCompileOptions

Impact:
- Reduces maintenance burden
- Makes adding new formats easier
- Eliminates potential panic sites
- Improves code readability

Files involved:
- src/rs/cli.rs (890 lines, major deduplication target)
- src/rs/formats/pdf.rs (518 lines)
- src/rs/formats/html/mod.rs (182 lines)
- src/rs/formats/epub/mod.rs (416 lines)
- src/rs/compile.rs (594 lines)
- src/rs/error.rs (103 lines)
- src/rs/world.rs
- src/rs/spine.rs
```

---

## Phase 1: Foundation & Safety

### Task 1: `bd-refactor-1`

**Title:** "Extract file extension constants to central module"

**Type:** task

**Priority:** 2 (medium)

**Dependencies:** None

**Description:**
```
Create src/rs/constants.rs module with all hardcoded file extensions.

Current state: Extensions hardcoded in 20+ locations:
- ".typ" in 8+ files (compile.rs:160, pdf.rs:193,227,320, links.rs:68,109)
- ".html" in 9+ locations across 4 files
- ".pdf", ".xhtml", ".epub" scattered throughout

Implementation:
1. Create src/rs/constants.rs:
   pub const TYP_EXT: &str = ".typ";
   pub const PDF_EXT: &str = ".pdf";
   pub const HTML_EXT: &str = ".html";
   pub const XHTML_EXT: &str = ".xhtml";
   pub const EPUB_EXT: &str = ".epub";

2. Add to lib.rs:
   mod constants;
   pub use constants::*;

3. Replace all hardcoded strings:
   - Search for ".typ" → replace with TYP_EXT
   - Search for ".pdf" → replace with PDF_EXT
   - Search for ".html" → replace with HTML_EXT
   etc.

Files to modify:
- NEW: src/rs/constants.rs
- src/rs/lib.rs
- src/rs/compile.rs (lines 160, 134)
- src/rs/formats/pdf.rs (lines 193, 227, 320)
- src/rs/formats/html/mod.rs
- src/rs/formats/epub/mod.rs
- src/rs/postprocess/links.rs (lines 68, 109)
- tests/helpers/comparison.rs

Testing: Run full test suite to ensure no behavioral changes.
```

### Task 2: `bd-refactor-2`

**Title:** "Extract shared regex patterns to lazy_static constants"

**Type:** task

**Priority:** 2 (medium)

**Dependencies:** bd-refactor-1

**Description:**
```
Deduplicate regex patterns currently defined in multiple files.

Current state:
- Link pattern r#"#link\("([^"]+)"\)(\[[^\]]+\]|,\s*[^)]+)"# defined in:
  * compile.rs:134
  * pdf.rs:120,202 (two separate instances)
- Href pattern in postprocess/links.rs:37

Implementation:
1. Extend src/rs/constants.rs with regex patterns using lazy_static:
   use lazy_static::lazy_static;
   use regex::Regex;

   lazy_static! {
       pub static ref TYPST_LINK_PATTERN: Regex =
           Regex::new(r#"#link\("([^"]+)"\)(\[[^\]]+\]|,\s*[^)]+)"#)
               .expect("invalid regex pattern");

       pub static ref HTML_HREF_PATTERN: Regex =
           Regex::new(r#"href="([^"]+)""#)
               .expect("invalid regex pattern");
   }

2. Update Cargo.toml dependencies if needed:
   lazy_static = "1.4"

3. Replace regex definitions:
   - compile.rs:134 → use TYPST_LINK_PATTERN
   - pdf.rs:120,202 → use TYPST_LINK_PATTERN
   - postprocess/links.rs:37 → use HTML_HREF_PATTERN

Files to modify:
- src/rs/constants.rs (extend)
- src/rs/compile.rs (line 134)
- src/rs/formats/pdf.rs (lines 120, 202)
- src/rs/postprocess/links.rs (line 37)
- Cargo.toml (add lazy_static if not present)

Testing: Run full test suite, especially link transformation tests.
```

### Task 3: `bd-refactor-3`

**Title:** "Replace production .unwrap() calls with proper error handling"

**Type:** task

**Priority:** 0 (critical)

**Dependencies:** None

**Description:**
```
Eliminate panic risks by replacing .unwrap() with proper error propagation.

Current state: 400+ .unwrap() calls, many in production code paths.

Critical locations to fix:
1. compile.rs:156 - re.captures(mat.as_str()).unwrap()
   → Use .ok_or_else(|| RheoError::ParseError("invalid link format"))?

2. spine.rs:105 - p.file_name().unwrap()
   → Add proper error for paths without filenames

3. formats/epub/xhtml.rs:95,100 - chars.next().unwrap()
   → Check chars.next() before using

4. formats/epub/mod.rs:76,84 - outline.take().unwrap()
   → Return error if outline is None

5. Path operations throughout:
   - .file_name().unwrap() → use ok_or_else with descriptive error
   - .to_str().unwrap() → handle non-UTF8 paths

Implementation strategy:
- Focus on production code (not test code using .expect())
- Add descriptive error variants to RheoError enum as needed
- Use ? operator for error propagation
- Only allow .unwrap() in truly infallible operations (document why)

Files to modify:
- src/rs/error.rs (add new error variants)
- src/rs/compile.rs (line 156)
- src/rs/spine.rs (line 105)
- src/rs/formats/epub/xhtml.rs (lines 95, 100)
- src/rs/formats/epub/mod.rs (lines 76, 84)
- src/rs/formats/pdf.rs (review unwraps)
- src/rs/formats/html/mod.rs (review unwraps)

Testing:
- Run full test suite
- Consider adding tests for error cases (invalid paths, malformed links)
```

### Task 4: `bd-refactor-4`

**Title:** "Create CompilationResults type for format result aggregation"

**Type:** task

**Priority:** 2 (medium)

**Dependencies:** None

**Description:**
```
Replace manual result counting with type-safe aggregation structure.

Current state: Manual tracking with 6 variables:
- pdf_succeeded, pdf_failed
- html_succeeded, html_failed
- epub_succeeded, epub_failed

Repeated in 2 functions (perform_compilation, perform_compilation_incremental).

Implementation:
1. Create src/rs/results.rs:
   use std::collections::HashMap;
   use crate::OutputFormat;

   #[derive(Debug, Default)]
   pub struct CompilationResults {
       results: HashMap<OutputFormat, FormatResult>,
   }

   #[derive(Debug, Default, Clone, Copy)]
   pub struct FormatResult {
       pub succeeded: usize,
       pub failed: usize,
   }

   impl CompilationResults {
       pub fn new() -> Self { ... }
       pub fn record_success(&mut self, format: OutputFormat) { ... }
       pub fn record_failure(&mut self, format: OutputFormat) { ... }
       pub fn get(&self, format: OutputFormat) -> &FormatResult { ... }
       pub fn has_failures(&self) -> bool { ... }
       pub fn log_summary(&self, requested_formats: &[OutputFormat]) { ... }
   }

2. Add to lib.rs:
   mod results;
   pub use results::{CompilationResults, FormatResult};

3. Replace manual counters in cli.rs:
   - Lines 220-287 (per-file compilation)
   - Lines 290-327 (merged compilation)
   - Lines 333-382 (result logging)
   - Same pattern in perform_compilation_incremental

Files to modify:
- NEW: src/rs/results.rs
- src/rs/lib.rs
- src/rs/cli.rs (lines 210-408, 425-571)

Expected reduction: ~80 lines of manual counting and logging logic.

Testing: Ensure compilation output messages are unchanged.
```

---

## Phase 2: Core Abstractions

### Task 5: `bd-refactor-5`

**Title:** "Design FormatCompiler trait interface"

**Type:** task

**Priority:** 1 (high)

**Dependencies:** bd-refactor-1, bd-refactor-2

**Description:**
```
Design (don't implement yet) the FormatCompiler trait to unify format compilation.

Goal: Create trait interface that all formats (PDF, HTML, EPUB) will implement.

Design considerations:
1. Support both fresh and incremental compilation
2. Handle format-specific config types
3. Support optional per-file vs merged modes
4. Maintain existing error handling via codespan

Proposed interface (src/rs/formats/compiler.rs):

pub trait FormatCompiler {
    type Config;

    /// File extension without dot (e.g., "pdf")
    fn extension(&self) -> &'static str;

    /// Whether this format supports per-file compilation with given config
    fn supports_per_file(&self, config: &Self::Config) -> bool;

    /// Compile using fresh World
    fn compile_fresh(
        &self,
        input: &Path,
        output: &Path,
        root: &Path,
        repo_root: &Path,
        config: &Self::Config,
    ) -> Result<()>;

    /// Compile using existing World (incremental)
    fn compile_incremental(
        &self,
        world: &mut RheoWorld,
        input: &Path,
        output: &Path,
        config: &Self::Config,
    ) -> Result<()>;
}

// Format-specific types
pub struct PdfCompiler;
pub struct HtmlCompiler;
pub struct EpubCompiler;

// Dispatch enum
pub enum FormatCompilerInstance {
    Pdf(PdfCompiler),
    Html(HtmlCompiler),
    Epub(EpubCompiler),
}

impl FormatCompilerInstance {
    pub fn from_format(format: OutputFormat) -> Self { ... }
}

Files to create:
- src/rs/formats/compiler.rs (trait definition)

Files to review for design:
- src/rs/formats/pdf.rs (current API)
- src/rs/formats/html/mod.rs (current API)
- src/rs/formats/epub/mod.rs (current API)

Deliverable: Trait definition file ready for implementation tasks.
No behavioral changes in this task.
```

### Task 6: `bd-refactor-6`

**Title:** "Implement FormatCompiler for PdfCompiler"

**Type:** task

**Priority:** 1 (high)

**Dependencies:** bd-refactor-5

**Description:**
```
Implement the FormatCompiler trait for PDF compilation.

Implementation in src/rs/formats/pdf.rs:

use super::compiler::FormatCompiler;

pub struct PdfCompiler;

impl FormatCompiler for PdfCompiler {
    type Config = Option<PdfConfig>;

    fn extension(&self) -> &'static str {
        "pdf"
    }

    fn supports_per_file(&self, config: &Self::Config) -> bool {
        // Only per-file if not merging
        config.as_ref().and_then(|c| c.merge.as_ref()).is_none()
    }

    fn compile_fresh(
        &self,
        input: &Path,
        output: &Path,
        root: &Path,
        repo_root: &Path,
        config: &Self::Config,
    ) -> Result<()> {
        // Route to existing compile_pdf_single_impl_fresh or compile_pdf_merged_impl_fresh
        // based on config.merge
        match config.as_ref().and_then(|c| c.merge.as_ref()) {
            Some(merge_config) => {
                compile_pdf_merged_impl_fresh(input, output, root, repo_root, merge_config)
            }
            None => {
                compile_pdf_single_impl_fresh(input, output, root, repo_root)
            }
        }
    }

    fn compile_incremental(
        &self,
        world: &mut RheoWorld,
        input: &Path,
        output: &Path,
        config: &Self::Config,
    ) -> Result<()> {
        // Route to existing compile_pdf_single_impl or compile_pdf_merged_impl
        match config.as_ref().and_then(|c| c.merge.as_ref()) {
            Some(merge_config) => {
                compile_pdf_merged_impl(world, input, output, merge_config)
            }
            None => {
                compile_pdf_single_impl(world, input, output)
            }
        }
    }
}

Refactor existing code:
- Keep compile_pdf_single_impl, compile_pdf_merged_impl (internal)
- Keep compile_pdf_single_impl_fresh, compile_pdf_merged_impl_fresh (internal)
- Update compile_pdf_new() to use PdfCompiler trait methods
- Mark internal functions as pub(crate) instead of pub

Files to modify:
- src/rs/formats/pdf.rs

Testing:
- Run PDF compilation tests
- Ensure both single-file and merged modes work
- Test incremental compilation in watch mode
```

### Task 7: `bd-refactor-7`

**Title:** "Implement FormatCompiler for HtmlCompiler"

**Type:** task

**Priority:** 1 (high)

**Dependencies:** bd-refactor-5

**Description:**
```
Implement the FormatCompiler trait for HTML compilation.

Implementation in src/rs/formats/html/mod.rs:

use super::compiler::FormatCompiler;

pub struct HtmlCompiler;

impl FormatCompiler for HtmlCompiler {
    type Config = HtmlOptions;

    fn extension(&self) -> &'static str {
        "html"
    }

    fn supports_per_file(&self, _config: &Self::Config) -> bool {
        // HTML always supports per-file
        true
    }

    fn compile_fresh(
        &self,
        input: &Path,
        output: &Path,
        root: &Path,
        repo_root: &Path,
        config: &Self::Config,
    ) -> Result<()> {
        compile_html_impl_fresh(input, output, root, repo_root, config)
    }

    fn compile_incremental(
        &self,
        world: &mut RheoWorld,
        input: &Path,
        output: &Path,
        config: &Self::Config,
    ) -> Result<()> {
        compile_html_impl(world, input, output, config)
    }
}

Refactor existing code:
- Keep compile_html_impl, compile_html_impl_fresh (internal)
- Update compile_html_new() to use HtmlCompiler trait methods
- Mark internal functions as pub(crate)

Files to modify:
- src/rs/formats/html/mod.rs

Testing:
- Run HTML compilation tests
- Test CSS/font injection
- Test link transformation
- Test incremental compilation in watch mode
```

### Task 8: `bd-refactor-8`

**Title:** "Implement FormatCompiler for EpubCompiler"

**Type:** task

**Priority:** 1 (high)

**Dependencies:** bd-refactor-5

**Description:**
```
Implement the FormatCompiler trait for EPUB compilation.

Implementation in src/rs/formats/epub/mod.rs:

use super::compiler::FormatCompiler;

pub struct EpubCompiler;

impl FormatCompiler for EpubCompiler {
    type Config = EpubOptions;

    fn extension(&self) -> &'static str {
        "epub"
    }

    fn supports_per_file(&self, _config: &Self::Config) -> bool {
        // EPUB never supports per-file (always merged)
        false
    }

    fn compile_fresh(
        &self,
        input: &Path,
        output: &Path,
        root: &Path,
        repo_root: &Path,
        config: &Self::Config,
    ) -> Result<()> {
        // Note: EPUB compilation currently doesn't use input param directly
        // It compiles all files from spine
        compile_epub_impl_fresh(output, root, repo_root, config)
    }

    fn compile_incremental(
        &self,
        world: &mut RheoWorld,
        input: &Path,
        output: &Path,
        config: &Self::Config,
    ) -> Result<()> {
        // TODO: Implement incremental EPUB (currently not supported)
        // For now, delegate to fresh compilation
        self.compile_fresh(input, output, &world.root(), &world.repo_root(), config)
    }
}

Refactor existing code:
- Extract fresh compilation logic to compile_epub_impl_fresh (internal)
- Update compile_epub_new() to use EpubCompiler trait methods
- Add TODO for incremental EPUB support

Files to modify:
- src/rs/formats/epub/mod.rs

Testing:
- Run EPUB compilation tests
- Test default title/spine inference
- Test with explicit rheo.toml config
```

### Task 9: `bd-refactor-9`

**Title:** "Add OutputFormat::compiler() dispatch method"

**Type:** task

**Priority:** 2 (medium)

**Dependencies:** bd-refactor-6, bd-refactor-7, bd-refactor-8

**Description:**
```
Add method to OutputFormat enum for getting appropriate compiler instance.

Implementation in src/rs/lib.rs (around line 27):

use crate::formats::compiler::{FormatCompilerInstance, PdfCompiler, HtmlCompiler, EpubCompiler};

impl OutputFormat {
    pub fn compiler(&self) -> FormatCompilerInstance {
        match self {
            OutputFormat::Pdf => FormatCompilerInstance::Pdf(PdfCompiler),
            OutputFormat::Html => FormatCompilerInstance::Html(HtmlCompiler),
            OutputFormat::Epub => FormatCompilerInstance::Epub(EpubCompiler),
        }
    }

    pub fn supports_per_file(&self, config: &RheoConfig) -> bool {
        match self {
            OutputFormat::Pdf => {
                let compiler = PdfCompiler;
                compiler.supports_per_file(&config.pdf)
            }
            OutputFormat::Html => {
                let compiler = HtmlCompiler;
                let opts = HtmlOptions::from_config(config);
                compiler.supports_per_file(&opts)
            }
            OutputFormat::Epub => false,
        }
    }
}

This centralizes format dispatch logic currently scattered across cli.rs.

Files to modify:
- src/rs/lib.rs (OutputFormat impl)
- src/rs/formats/compiler.rs (FormatCompilerInstance)

Benefits:
- Single source of truth for format->compiler mapping
- Reduces match statements in caller code
- Makes adding new formats cleaner

Testing: Ensure existing compilation behavior unchanged.
```

---

## Phase 3: Deduplication

### Task 10: `bd-refactor-10`

**Title:** "Deduplicate perform_compilation functions using FormatCompiler"

**Type:** task

**Priority:** 1 (high)

**Dependencies:** bd-refactor-9, bd-refactor-4

**Description:**
```
Eliminate ~200 lines of duplication between perform_compilation() and
perform_compilation_incremental() by using FormatCompiler trait.

Current state:
- perform_compilation() - lines 210-408 in cli.rs
- perform_compilation_incremental() - lines 425-571 in cli.rs
- Near-identical loops for PDF, HTML, EPUB with manual counting

New unified approach:

fn perform_compilation(
    mode: CompilationMode,
    typ_files: &[PathBuf],
    config: &ProjectConfig,
    output_config: &OutputConfig,
    requested_formats: &[OutputFormat],
) -> Result<()> {
    let mut results = CompilationResults::new();

    // Per-file compilation
    let per_file_formats = get_per_file_formats(&config.config, requested_formats);
    for typ_file in typ_files {
        for format in &per_file_formats {
            let compiler = format.compiler();
            let output_path = output_config
                .dir_for_format(*format)
                .join(&filename)
                .with_extension(compiler.extension());

            let result = match &mode {
                CompilationMode::Fresh { root, repo_root } => {
                    compiler.compile_fresh(typ_file, &output_path, root, repo_root, config)
                }
                CompilationMode::Incremental { world } => {
                    compiler.compile_incremental(world, typ_file, &output_path, config)
                }
            };

            match result {
                Ok(_) => results.record_success(*format),
                Err(e) => {
                    error!(file = %typ_file.display(), error = %e);
                    results.record_failure(*format);
                }
            }
        }
    }

    // Merged compilation (similar pattern)
    // ...

    results.log_summary(requested_formats);
    if results.has_failures() {
        Err(RheoError::CompilationFailed)
    } else {
        Ok(())
    }
}

Where CompilationMode is:
enum CompilationMode<'a> {
    Fresh {
        root: PathBuf,
        repo_root: PathBuf,
    },
    Incremental {
        world: &'a mut RheoWorld,
    },
}

Files to modify:
- src/rs/cli.rs (lines 210-571)
- NEW: CompilationMode enum (could be in cli.rs or separate module)

Expected reduction: ~200 lines of duplicate code.

Testing:
- Run full test suite
- Test both compile command (fresh) and watch mode (incremental)
- Verify error messages unchanged
```

### Task 11: `bd-refactor-11`

**Title:** "Add builder pattern for RheoCompileOptions"

**Type:** task

**Priority:** 2 (medium)

**Dependencies:** bd-refactor-10

**Description:**
```
Replace manual struct construction with ergonomic builder pattern.

Current usage in cli.rs (appears ~20 times):
let options = RheoCompileOptions::new(typ_file, &output_path, &compilation_root, &repo_root);

let options = RheoCompileOptions::incremental(
    world,
    typ_file,
    &output_path,
    &compilation_root,
    &repo_root,
);

Proposed builder API:
let options = RheoCompileOptions::builder()
    .input(typ_file)
    .output(&output_path)
    .root(&compilation_root)
    .repo_root(&repo_root)
    .build();

let options = RheoCompileOptions::builder()
    .input(typ_file)
    .output(&output_path)
    .root(&compilation_root)
    .repo_root(&repo_root)
    .world(world)  // Optional for incremental
    .build();

Implementation in src/rs/compile.rs:

pub struct RheoCompileOptionsBuilder {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    root: Option<PathBuf>,
    repo_root: Option<PathBuf>,
    world: Option<Box<RheoWorld>>,
}

impl RheoCompileOptions {
    pub fn builder() -> RheoCompileOptionsBuilder {
        RheoCompileOptionsBuilder::default()
    }
}

impl RheoCompileOptionsBuilder {
    pub fn input(mut self, path: impl Into<PathBuf>) -> Self { ... }
    pub fn output(mut self, path: impl Into<PathBuf>) -> Self { ... }
    pub fn root(mut self, path: impl Into<PathBuf>) -> Self { ... }
    pub fn repo_root(mut self, path: impl Into<PathBuf>) -> Self { ... }
    pub fn world(mut self, world: RheoWorld) -> Self { ... }

    pub fn build(self) -> Result<RheoCompileOptions> {
        Ok(RheoCompileOptions {
            input: self.input.ok_or(RheoError::BuilderError("input required"))?,
            output: self.output.ok_or(RheoError::BuilderError("output required"))?,
            root: self.root.ok_or(RheoError::BuilderError("root required"))?,
            repo_root: self.repo_root.ok_or(RheoError::BuilderError("repo_root required"))?,
            world: self.world,
        })
    }
}

Files to modify:
- src/rs/compile.rs (add builder)
- src/rs/error.rs (add BuilderError variant if needed)
- src/rs/cli.rs (update ~20 call sites)

Benefits:
- Self-documenting parameter names
- Compile-time checking of required fields
- Easy to add optional parameters later

Testing: Ensure compilation behavior unchanged.
```

### Task 12: `bd-refactor-12`

**Title:** "Extract get_per_file_formats logic to OutputFormat methods"

**Type:** task

**Priority:** 3 (low)

**Dependencies:** bd-refactor-9

**Description:**
```
Move format-specific per-file logic from cli.rs helper to OutputFormat impl.

Current state (cli.rs:165-196):
fn get_per_file_formats(config: &RheoConfig, requested: &[OutputFormat]) -> Vec<OutputFormat> {
    let mut formats = Vec::new();
    for &format in requested {
        match format {
            OutputFormat::Html => { formats.push(format); }
            OutputFormat::Pdf => {
                if config.pdf.merge.is_none() {
                    formats.push(format);
                }
            }
            OutputFormat::Epub => { /* never */ }
        }
    }
    formats
}

New approach using trait method from bd-refactor-9:

impl OutputFormat {
    /// Already added in bd-refactor-9
    pub fn supports_per_file(&self, config: &RheoConfig) -> bool { ... }
}

fn get_per_file_formats(config: &RheoConfig, requested: &[OutputFormat]) -> Vec<OutputFormat> {
    requested
        .iter()
        .copied()
        .filter(|format| format.supports_per_file(config))
        .collect()
}

Files to modify:
- src/rs/cli.rs (lines 165-196)

Benefits:
- Centralized format behavior
- Easier to add new formats
- More functional style

Testing: Ensure per-file vs merged compilation behavior unchanged.
```

### Task 13: `bd-refactor-13`

**Title:** "Simplify repeated conditional logging with helper function"

**Type:** task

**Priority:** 3 (low)

**Dependencies:** bd-refactor-4

**Description:**
```
Extract repeated logging pattern to CompilationResults::log_summary().

Current state (cli.rs:333-382): Identical pattern for PDF, HTML, EPUB:
if formats.contains(&OutputFormat::Pdf) {
    if pdf_failed > 0 {
        warn!(failed = pdf_failed, succeeded = pdf_succeeded, "PDF compilation completed with failures");
    } else {
        info!(succeeded = pdf_succeeded, "PDF compilation completed");
    }
}

This is already handled by bd-refactor-4's CompilationResults::log_summary(),
but this task ensures all call sites are updated.

Review and update:
- Ensure CompilationResults::log_summary() produces same output format
- Remove manual logging blocks in cli.rs
- Verify watch mode also uses new logging

Files to modify:
- src/rs/results.rs (log_summary implementation)
- src/rs/cli.rs (remove manual logging at lines 333-382, 539-571)

Expected reduction: ~50 lines of logging boilerplate.

Testing: Compare log output before/after to ensure format unchanged.
```

---

## Phase 4: Polish

### Task 14: `bd-refactor-14`

**Title:** "Add type-safe path handling utilities"

**Type:** task

**Priority:** 3 (low)

**Dependencies:** bd-refactor-3

**Description:**
```
Create helper utilities for common path operations to avoid unwrap().

Current issues (from bd-refactor-3):
- .file_name().unwrap().to_str().unwrap() chains
- No consistent error handling for non-UTF8 paths
- Manual path manipulation prone to errors

Create src/rs/path_utils.rs:

use std::path::{Path, PathBuf};
use crate::error::RheoError;

pub trait PathExt {
    /// Get file name as str, returning error if None or non-UTF8
    fn file_name_str(&self) -> Result<&str>;

    /// Get file stem as str, returning error if None or non-UTF8
    fn file_stem_str(&self) -> Result<&str>;

    /// Replace extension, preserving file name
    fn with_extension_safe(&self, ext: &str) -> Result<PathBuf>;
}

impl PathExt for Path {
    fn file_name_str(&self) -> Result<&str> {
        self.file_name()
            .ok_or_else(|| RheoError::InvalidPath(format!("path has no filename: {}", self.display())))?
            .to_str()
            .ok_or_else(|| RheoError::InvalidPath(format!("non-UTF8 path: {}", self.display())))
    }

    // ... similar for other methods
}

Usage replaces:
path.file_name().unwrap().to_str().unwrap()
→
path.file_name_str()?

Files to create:
- src/rs/path_utils.rs

Files to modify:
- src/rs/lib.rs (add mod path_utils)
- src/rs/spine.rs (use PathExt)
- src/rs/cli.rs (use PathExt)
- tests/helpers/fixtures.rs (use PathExt)

Benefits:
- Consistent error handling
- Self-documenting code
- Reduces unwrap() usage

Testing: Run full test suite.
```

### Task 15: `bd-refactor-15`

**Title:** "Add validation trait for configuration structs"

**Type:** task

**Priority:** 4 (backlog)

**Dependencies:** None

**Description:**
```
Create ValidateConfig trait for consistent configuration validation.

Current state: Validation scattered across usage sites, inconsistent patterns.

Create src/rs/validation.rs:

pub trait ValidateConfig {
    fn validate(&self) -> Result<()>;
}

impl ValidateConfig for PdfConfig {
    fn validate(&self) -> Result<()> {
        if let Some(merge) = &self.merge {
            merge.validate()?;
        }
        Ok(())
    }
}

impl ValidateConfig for Merge {
    fn validate(&self) -> Result<()> {
        if self.spine.patterns.is_empty() {
            return Err(RheoError::ConfigError("merge spine cannot be empty"));
        }
        // Validate patterns are valid globs
        for pattern in &self.spine.patterns {
            glob::Pattern::new(pattern)
                .map_err(|e| RheoError::ConfigError(format!("invalid glob pattern: {}", e)))?;
        }
        Ok(())
    }
}

// Similar for HtmlConfig, EpubConfig

Usage in config loading (config.rs):
let config = RheoConfig::load(path)?;
config.pdf.validate()?;
config.html.validate()?;
config.epub.validate()?;

Files to create:
- src/rs/validation.rs

Files to modify:
- src/rs/config.rs (call validate after loading)
- src/rs/lib.rs (add mod validation)
- src/rs/error.rs (ensure ConfigError variant exists)

Benefits:
- Early error detection
- Consistent validation
- Self-documenting config requirements

Testing:
- Add tests for invalid configs
- Ensure existing valid configs still work
```

---

## Implementation Order

The tasks should be implemented in dependency order:

1. **Phase 1** (can be parallel):
   - bd-refactor-1 (constants)
   - bd-refactor-3 (unwrap fixes) - can start independently
   - bd-refactor-4 (results type) - can start independently

2. bd-refactor-2 (regex) - after bd-refactor-1

3. **Phase 2** (sequential):
   - bd-refactor-5 (trait design)
   - bd-refactor-6, 7, 8 (implementations) - can be parallel
   - bd-refactor-9 (dispatch) - after 6, 7, 8

4. **Phase 3**:
   - bd-refactor-10 (deduplication) - after Phase 2 and bd-refactor-4
   - bd-refactor-11 (builder) - after bd-refactor-10
   - bd-refactor-12, 13 (cleanups) - can be parallel

5. **Phase 4** (independent):
   - bd-refactor-14 (path utils)
   - bd-refactor-15 (validation) - optional, can defer

## Expected Impact

### Lines of Code Reduction
- Compilation loop deduplication: ~200 lines
- Format dispatch logic: ~40 lines
- Result aggregation: ~80 lines
- Logging boilerplate: ~50 lines
- **Total reduction: ~370 lines**

### Code Quality Improvements
- Eliminate 400+ potential panic sites
- Centralize 20+ hardcoded string literals
- Unify 3 duplicate regex definitions
- Add compile-time safety via traits
- Improve error messages

### Maintainability
- Adding new formats requires implementing 1 trait (vs touching 10+ files)
- Configuration changes centralized
- Path handling becomes consistent
- Builder pattern simplifies API usage

## Testing Strategy

For each task:
1. Run full test suite: `cargo test`
2. Run specific format tests if available
3. Manual testing of compile/watch commands
4. Verify log output unchanged (where applicable)

Integration testing:
- After Phase 2: Test all three formats work with trait-based compilation
- After Phase 3: Test both fresh and incremental compilation modes
- Final: Run examples (blog_site, blog_post, etc.) to ensure no regressions

## Notes for AI Implementation

- These tasks should be created in beads with the above details
- Use `--deps discovered-from:bd-refactor` to link tasks to epic
- Mark tasks with appropriate dependencies using `--deps blocks:bd-refactor-X`
- Update task status as work progresses
- If discovering new issues during implementation, create additional tasks linked to the epic

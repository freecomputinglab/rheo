# Rheo Integration Test Suite

This directory contains the integration test suite for rheo compilation. The tests verify that rheo correctly compiles Typst projects to PDF, HTML and EPUB formats.

## Test Structure

```
tests/
├── harness.rs                # Main test file with #[test_case] declarations
├── helpers/                  # Test helper modules
│   ├── mod.rs               # Module declarations
│   ├── fixtures.rs          # TestCase types (Directory and SingleFile)
│   ├── comparison.rs        # HTML/PDF comparison and validation
│   ├── reference.rs         # Reference generation
│   └── markers.rs           # Test marker parser for .typ files
├── ref/                     # Reference outputs (committed to git)
│   ├── examples/            # Project tests
│   │   └── blog_site/
│   │       ├── html/        # Reference HTML outputs
│   │       └── pdf/         # Reference PDF metadata (*.metadata.json)
│   ├── cases/               # Custom project tests
│   └── files/               # Single-file tests (NEW)
│       └── <hash>/          # Hash-based directory for each file
│           └── <filename>/
│               ├── html/
│               └── pdf/
└── store/                   # Temporary test outputs (gitignored)
```

## Running Tests

### Run all tests
```bash
cargo test
```

### Run integration tests only
```bash
cargo test --test integration_test
```

### Run with verbose output
```bash
cargo test -- --nocapture
```

## Font Consistency

To ensure tests produce identical output across different environments (local machines and CI), tests automatically use only Typst's embedded fonts. This prevents font-related rendering differences that cause page count and layout variations.

**The test harness automatically sets `TYPST_IGNORE_SYSTEM_FONTS=1`**, so you can simply run:
```bash
cargo test --test harness
```

**Why this matters:**
- Different machines have different system fonts installed
- Font metrics (line height, character width) affect text layout
- Layout differences cause page breaks to vary → different page counts
- CI (Ubuntu) has different fonts than macOS/Windows

**Typst's embedded fonts** (New Computer Modern, Libertinus) are:
- Bundled with the Typst compiler
- Identical across all platforms
- Deterministic in rendering behavior

**Implementation:** The environment variable is automatically passed to all `cargo run` subprocess invocations in `tests/harness.rs`.

**Note:** This only affects tests. Normal `rheo compile` commands still use system fonts as expected.

## Updating Reference Outputs

When you make intentional changes to rheo's output, update the reference files:

### Update all references
```bash
UPDATE_REFERENCES=1 cargo test --test integration_test
```

This will:
1. Compile all test projects
2. Copy HTML outputs to `tests/ref/<project>/html/`
3. Extract PDF metadata to `tests/ref/<project>/pdf/*.metadata.json`

After updating, commit the changed reference files to git.

## Test Filtering

### Run only HTML tests (across all projects that support HTML)
```bash
RUN_HTML_TESTS=1 cargo test --test integration_test
```

### Run only PDF tests (across all projects that support PDF)
```bash
RUN_PDF_TESTS=1 cargo test --test integration_test
```

### Run both formats explicitly
```bash
RUN_HTML_TESTS=1 RUN_PDF_TESTS=1 cargo test
```

### Increase diff output limit (default: 2000 chars)
```bash
RHEO_TEST_DIFF_LIMIT=10000 cargo test -- --nocapture
```

**Behavior**:
- **Default** (no env vars): Run all formats specified by project's `rheo.toml`
- **With env vars**: Filter to specified formats, respecting project capabilities
- Environment variables override project defaults but still respect what the project supports

## How Tests Work

Rheo supports two test modes: **Directory Tests** (full projects) and **Single-File Tests** (individual .typ files).

### Directory Mode Tests

1. **Discovery**: Finds all `examples/*/rheo.toml` projects
2. **Compilation**: Runs `cargo run -- compile <project-path>` for each project
3. **Verification**: Compares output against reference files:
   - **HTML**: Byte-for-byte comparison of HTML content and asset validation
   - **PDF**: Metadata comparison (page count exact, file size within 10% tolerance)

### Single-File Mode Tests (NEW)

Single-file tests allow testing individual .typ files without creating a full project structure.

**Adding test markers to .typ files**:
```typst
// @rheo:test
// @rheo:formats html,pdf
// @rheo:description Main blog index page with post listings

= My Document
Content here...
```

**Test marker syntax**:
- `// @rheo:test` (required) - Marks file as test case
- `// @rheo:formats <list>` (optional) - Comma-separated formats (html, pdf, epub). Default: html,pdf
- `// @rheo:description <text>` (optional) - Human-readable test description

**Declaring single-file tests** in `tests/harness.rs`:
```rust
#[test_case("file:examples/blog_site/content/index.typ")]
#[test_case("file:examples/blog_site/content/severance-ep-1.typ")]
fn run_test_case(name: &str) { ... }
```

**Reference storage**:
- Projects: `tests/ref/examples/<project>/html/`
- Single files: `tests/ref/files/<hash>/<filename>/html/`
- Hash prevents conflicts between files with the same name

**Running single-file tests**:
```bash
# Run specific single-file test
cargo test run_test_case_file_colonexamples_slashblog_site_slashcontent_slashindex_full_stoptyp

# Update references for single-file test
UPDATE_REFERENCES=1 cargo test run_test_case_file_colonexamples_slashblog_site_slashcontent_slashindex_full_stoptyp
```

### HTML Verification

- Compares HTML content byte-for-byte using unified diffs
- Validates that all expected assets (images, .typ files, CSS) are present
- Checks that no unexpected files appear in output
- Verifies exclusion patterns (e.g., blog_site excludes non-.typ files per `rheo.toml`)

### PDF Verification

- Extracts metadata: page count and file size
- Compares page count (must match exactly)
- Compares file size (must be within 10% tolerance)
- Verifies exclusion patterns (e.g., blog_site excludes `index.typ` from PDF)

### EPUB Testing

EPUB reference testing validates the structure and metadata of generated EPUB files using a lightweight metadata approach.

#### Approach

Unlike HTML (full content comparison) and similar to PDF (metadata only), EPUB testing uses metadata validation:

**What's Validated:**
- Title (from config or inferred from filename/directory)
- Language (from document metadata)
- Spine files (ordered list of content files, exact match)
- Navigation file existence (nav.xhtml)
- File size (10% tolerance)

**What's NOT Validated:**
- Timestamps (dcterms:modified changes every build)
- UUIDs (generated fresh each time)
- Exact XHTML content (already tested via HTML tests)

**Rationale:**
- EPUB content is derived from HTML compilation
- XHTML conversion is deterministic and unit tested
- Focus on structural integrity and configuration correctness
- Lightweight (no binary files in repo)

#### Reference Files

EPUB metadata stored as JSON:
```
tests/ref/
├── examples/
│   └── blog_post/
│       └── epub/
│           └── blog_post.metadata.json
└── cases/
    └── epub_explicit_config/
        └── epub/
            └── epub_explicit_config.metadata.json
```

Example metadata file:
```json
{
  "filetype": "epub",
  "file_size": 12453,
  "title": "Blog Post",
  "language": "en",
  "spine_files": ["portable_epubs.xhtml"],
  "has_nav": true
}
```

#### Running EPUB Tests

```bash
# Run only EPUB tests
RUN_EPUB_TESTS=1 cargo test --test harness

# Run specific EPUB test
cargo test run_test_case_examples_slashblog_post -- --nocapture

# Update EPUB references
UPDATE_REFERENCES=1 RUN_EPUB_TESTS=1 cargo test --test harness

# Run all formats (HTML, PDF, EPUB)
cargo test --test harness
```

#### When to Update References

Update EPUB references when:
- Changing EPUB title inference logic
- Changing spine ordering logic
- Changing EPUB compilation configuration
- Adding new EPUB test cases

DO NOT update for:
- Minor formatting changes (within 10% file size tolerance)
- Timestamp/UUID changes (not validated)

#### Troubleshooting

**File size mismatch beyond 10% tolerance:**
- Indicates significant structural change
- Review EPUB configuration or spine changes
- Update reference if intentional change

**Spine order mismatch:**
- Check rheo.toml spine configuration
- Verify file naming for default lexicographic sorting
- Update reference if intentional change

**Title/language mismatch:**
- Check rheo.toml [epub] configuration
- Verify document language metadata
- Update reference if intentional change

## Adding New Tests

### Add a new project test

1. Create a new project directory in `examples/`
2. Add a `rheo.toml` config file
3. Add `.typ` source files
4. Run `UPDATE_REFERENCES=1 cargo test` to generate references
5. Commit the reference files to git

### Test exclusions automatically

PDF and HTML exclusion patterns are tested automatically via reference validation:

- **PDF**: `validate_pdf_assets()` ensures actual PDFs match reference metadata files exactly
- **HTML**: `validate_html_assets()` ensures actual HTML files match reference files exactly

When you change exclusion patterns in `rheo.toml`:
1. Clean and compile: `cargo run -- clean examples/project && cargo run -- compile examples/project`
2. Update references: `UPDATE_REFERENCES=1 cargo test`
3. Tests will now fail if exclusions aren't respected

## Troubleshooting

### Test fails with "reference not found"

Run `UPDATE_REFERENCES=1 cargo test` to generate missing references.

### HTML content mismatch

The test will show an improved unified diff with:
- **Statistics**: Insertion/deletion counts
- **Diff preview**: First N characters (configurable via `RHEO_TEST_DIFF_LIMIT`)
- **Update command**: Test-specific command to update references
- **Full diff option**: Command to see complete diff if truncated

Example error:
```
HTML content mismatch for tests/ref/examples/blog_site/html/index.html

Diff: 12 insertions(+), 8 deletions(-), 145 lines unchanged

 <div class='content'>
-  <h1>Old Title</h1>
+  <h1>New Title</h1>
+  <p>Additional paragraph</p>
 </div>

... (showing first 2000 chars of 5000 bytes total)

To update this reference, run:
  UPDATE_REFERENCES=1 cargo test run_test_case_examples_slashblog_site -- --nocapture

Or to see full diff:
  RHEO_TEST_DIFF_LIMIT=10000 cargo test run_test_case_examples_slashblog_site -- --nocapture
```

Common causes:
- Typst version changed (update references)
- Intentional output change (update references)
- Unintentional regression (fix the code)

### PDF metadata mismatch

Enhanced error messages now show:
- **Page count changes**: Shows exact difference (e.g., "3 pages added")
- **File size changes**: Shows percentage difference
- **Context**: Suggests this may indicate content or formatting changes

Example error:
```
PDF metadata mismatch:
  - Page count: expected 16, got 15 (1 pages removed)
  - File size: expected 24560 bytes, got 27200 bytes (11% diff, beyond 10% tolerance)

This may indicate a change in content or formatting.
```

Common causes:
- Typst version changed rendering (update references if expected)
- Page count changed (verify this is intentional)
- File size changed significantly (check for regression)

## Reference File Management

- **HTML references**: Full HTML files and assets committed to git
- **PDF references**: Metadata JSON only (page count, file size)
  - PDFs themselves are NOT committed (too large, binary)
  - Metadata provides sufficient validation for most cases
- **Update policy**: Update references when making intentional changes to output format

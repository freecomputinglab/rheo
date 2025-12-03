# Rheo Integration Test Suite

This directory contains the integration test suite for rheo compilation. The tests verify that rheo correctly compiles Typst projects to PDF, HTML and EPUB formats.

## Test Structure

```
tests/
├── integration_test.rs       # Main test file
├── helpers/                  # Test helper modules
│   ├── mod.rs               # Module declarations
│   ├── fixtures.rs          # TestCase types and setup/cleanup
│   ├── comparison.rs        # HTML/PDF comparison and validation
│   └── reference.rs         # Reference generation
├── ref/                     # Reference outputs (committed to git)
│   └── blog_site/
│       ├── html/           # Reference HTML outputs
│       └── pdf/            # Reference PDF metadata (*.metadata.json)
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

### Run only HTML tests
```bash
RUN_HTML_TESTS=1 cargo test --test integration_test
```

### Run only PDF tests
```bash
RUN_PDF_TESTS=1 cargo test --test integration_test
```

Note: By default, both HTML and PDF tests run unless you set these environment variables.

## How Tests Work

### Directory Mode Tests

1. **Discovery**: Finds all `examples/*/rheo.toml` projects
2. **Compilation**: Runs `cargo run -- compile <project-path>` for each project
3. **Verification**: Compares output against reference files:
   - **HTML**: Byte-for-byte comparison of HTML content and asset validation
   - **PDF**: Metadata comparison (page count exact, file size within 10% tolerance)

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

The test will show a unified diff. Common causes:
- Typst version changed (update references)
- Intentional output change (update references)
- Unintentional regression (fix the code)

### PDF metadata mismatch

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

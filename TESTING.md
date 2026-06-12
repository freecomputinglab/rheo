# Testing

Integration tests live in [freecomputinglab/rheo-tests](https://github.com/freecomputinglab/rheo-tests), cloned side-by-side at `../rheo-tests`. All integration test commands require `RHEO_MANIFEST=../rheo/Cargo.toml` to resolve path dependencies.

## Run All Tests (Unit Tests Only)
```bash
cargo test
```
Runs unit tests for all crates in the main repo. Integration tests run from the sibling rheo-tests clone.

## Run Individual Test Suites

### Unit Tests (all crates)
```bash
cargo test --lib
```

### Unit Tests (specific crate)
```bash
cargo test --lib -p rheo-core
cargo test --lib -p rheo
```

### Integration Tests (harness)
```bash
cd ../rheo-tests
RHEO_MANIFEST=../rheo/Cargo.toml cargo test --test harness
```
Runs integration tests against example projects and test cases.

### Compat Tests (skip by default)
```bash
cd ../rheo-tests
RHEO_MANIFEST=../rheo/Cargo.toml cargo test --test compat
```
Runs compatibility tests against external Rheo projects (skip immediately unless `RUN_COMPAT_TESTS=1`).

### Compat Tests (actually execute)
```bash
cd ../rheo-tests
RUN_COMPAT_TESTS=1 RHEO_MANIFEST=../rheo/Cargo.toml cargo test --test compat
```
Clones external repos and compiles them (~7 seconds).

## Format-Specific Tests
```bash
cd ../rheo-tests
RHEO_MANIFEST=../rheo/Cargo.toml RUN_HTML_TESTS=1 cargo test --test harness
RHEO_MANIFEST=../rheo/Cargo.toml RUN_PDF_TESTS=1 cargo test --test harness
RHEO_MANIFEST=../rheo/Cargo.toml RUN_EPUB_TESTS=1 cargo test --test harness
```
Only run tests targeting HTML/PDF/EPUB formats respectively.

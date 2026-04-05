# Testing

## Run All Tests
```bash
cargo test
```
Runs unit tests, integration tests, compat tests (skip unless `RUN_COMPAT_TESTS=1`), and doc tests.

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
cargo test -p rheo-tests --test harness
```
Runs 42 integration tests against example projects and test cases.

### Compat Tests (skip by default)
```bash
cargo test -p rheo-tests --test compat
```
Runs 5 compatibility tests against external Rheo projects (skip immediately unless `RUN_COMPAT_TESTS=1`).

### Compat Tests (actually execute)
```bash
RUN_COMPAT_TESTS=1 cargo test -p rheo-tests --test compat
```
Clones 5 external repos and compiles them (~7 seconds).

## Format-Specific Tests
```bash
RUN_HTML_TESTS=1 cargo test -p rheo-tests --test harness
RUN_PDF_TESTS=1 cargo test -p rheo-tests --test harness
RUN_EPUB_TESTS=1 cargo test -p rheo-tests --test harness
```
Only run tests targeting HTML/PDF/EPUB formats respectively.

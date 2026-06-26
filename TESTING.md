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

## CI and Cross-Repo Branch Pairing

Rheo's CI (`.github/workflows/ci.yml`) clones rheo-tests as a sibling before running integration tests. By default it clones `main`, but when a rheo branch has a paired rheo-tests branch, CI uses that instead.

### Convention

If a rheo-tests branch named `rheo/<branch>` exists, the rheo CI uses it automatically:

```
rheo branch              → rheo-tests branch used by rheo CI
────────────────────────────────────────────────────────────
feat/typst-v0.15.0       → rheo/feat/typst-v0.15.0 (if exists)
main                     → main (default)
fix/some-bug             → main (no paired branch)
```

The rheo-tests CI does the mirror: a rheo-tests branch named `rheo/<branch>` clones rheo at `<branch>`.

### PR workflow

When a rheo change requires updated test snapshots:

1. Create a rheo-tests branch named `rheo/<rheo-branch>`, update snapshots, open PR
2. Open the rheo PR — its CI finds and clones the paired rheo-tests branch automatically
3. Merge the rheo-tests PR first (snapshots land on `main`)
4. Merge the rheo PR — CI now uses rheo-tests `main` which already has the updated snapshots

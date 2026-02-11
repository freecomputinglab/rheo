# Releasing a new version of rheo

We aim to cut a release roughly every two weeks, whenever there are meaningful new features or fixes to ship.

## Steps to cut a release

1. **Update the version in `Cargo.toml`** to the new version number (e.g. `0.2.0`).

2. **Create a pull request** targeting `main`:
   - The PR title **must** be the version tag, e.g. `v0.2.0`.
   - Add the **`release`** label to the PR.

3. **Pre-release CI runs automatically** (`.github/workflows/pre-release.yml`):
   - Builds (and tests where possible) on all 6 supported platforms:
     - `x86_64-unknown-linux-gnu`
     - `aarch64-unknown-linux-gnu`
     - `x86_64-apple-darwin`
     - `aarch64-apple-darwin`
     - `x86_64-pc-windows-msvc`
     - `aarch64-pc-windows-msvc`
   - Validates the PR title matches `vX.Y.Z`.
   - Runs `cargo publish --workspace --dry-run` to verify crates.io readiness.

4. **Merge the PR** once CI is green and the changes are reviewed.

## What happens on merge

The release workflow (`.github/workflows/release.yml`) triggers automatically when a PR with the `release` label is merged to `main`. It runs three jobs in sequence:

1. **build-artifacts** — Compiles release binaries (`cargo build --locked --release`) for all 6 platforms and uploads them as workflow artifacts.

2. **publish-crates** — Publishes the crate to crates.io (`cargo publish --workspace`) and creates a git tag matching the PR title (e.g. `v0.2.0`) on the merge commit.

3. **publish-artifacts** — Creates a GitHub Release tagged with the version. Release notes are auto-generated from merged PR titles since the last release. Platform zip files (`rheo-{target}.zip`) are attached as release assets.

## After the release

- Review the auto-generated release notes on the GitHub Releases page and edit if needed.
- Verify the crate is live on [crates.io/crates/rheo](https://crates.io/crates/rheo).

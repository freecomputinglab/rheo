---
id: rh-cut-rheo-0-6-3-f6d543da
short-id: f6
title: Cut rheo 0.6.3
priority: 4
labels:
- fix-rheo-path-floor
deps:
- blocked-by:rh-rename-dot-marrow-is-epilogue-9b3bbd43
- blocked-by:rh-drop-an-index-left-childless-by-layering-d5f237fe
- blocked-by:rh-auto-index-must-not-refill-an-exclusion-c6b49937
closed: false
---
Every CI run on the `rookery` repository's `0.1.0` branch fails, and the fix is
a rheo release. This bird cuts it.

Touches: Cargo.toml, Cargo.lock, changelog.md

## Why

`rookery`'s demo and example projects declare where the `@rookery` namespace
resolves from with a `path` source:

```toml
[packages.rookery]
path = "../../../.."
```

That key was added by rheo PR #171, "Allow local custom namespaces", which
landed on `main` AFTER the `v0.6.2` tag — the tag sits on PR #170, "Locates a
ref-fetched package through the resolver, not a directory probe". No published
rheo release understands `path`, so a project using it dies at config load:

```
Error: ProjectConfig { message: "invalid rheo.toml: [packages.rookery]: set one of `repo` (a repository at a ref) or `releases` (a releases host)\n" }
```

This has been verified against the actual published `v0.6.2` binary, not
inferred: `crates/core/src/config/packages.rs` at the `v0.6.2` tag has a
two-armed `match (raw.repo, raw.releases)` and no `path` arm at all, while the
same file on `main` matches on `(raw.repo, raw.releases, raw.path)`.

A developer machine hides this completely, because a locally built `rheo` from
`main` still reports `rheo 0.6.2` — `Cargo.toml`'s version has not been bumped
since the release. The binary that has `path` and the binary that does not both
print the same version string. Bumping the version is therefore not only the
release mechanic, it is what makes the two distinguishable at all.

`main` is four commits past `v0.6.2`: PRs #171 (local custom namespaces), #172
(code quality in core and cli), #173 (Nix flake package fix) and #174
(incremental compile).

## Steps

1. Base the work on `main`, whose tip is "Support for incremental compile
   (#174)". Do NOT base it on the `feat/vertebra-prelude` bookmark or on any
   in-progress marrow-prelude work — that is unmerged and unrelated, and
   including it in a release commit ships it by accident.

2. In `/home/lox/code/_fcl/rheo/Cargo.toml`, change `0.6.2` to `0.6.3` in five
   places — the workspace package version at line 6, and the four path
   dependencies at lines 29-32:

   ```toml
   version = "0.6.2"                                            # line 6
   rheo-core = { path = "crates/core", version = "0.6.2" }      # line 29
   rheo-html = { path = "crates/html", version = "0.6.2" }      # line 30
   rheo-pdf  = { path = "crates/pdf",  version = "0.6.2" }      # line 31
   rheo-epub = { path = "crates/epub", version = "0.6.2" }      # line 32
   ```

   Lines 29-32 are shown here with their `=` aligned for readability; copy the
   version strings, not the spacing. Leave every other `0.6.2` in the
   repository alone — the ones under `.birds/` are fixture text inside a bird's
   own description, and the ones in `changelog.md` below line 90 are that
   release's own section heading and prose.

3. Refresh `Cargo.lock`. The five workspace crates carry the old version at
   lines 2629, 2650, 2686, 2706 and 2727. Do not hand-edit them:

   ```bash
   cd /home/lox/code/_fcl/rheo && cargo check --workspace
   ```

   That rewrites the lock as a side effect. `cargo check --workspace --locked`
   would instead fail, because the lock no longer matches the manifest — which
   is exactly the state this step exists to leave behind.

4. In `/home/lox/code/_fcl/rheo/changelog.md`, retitle line 1 from

   ```
   # Unreleased — user-visible changes
   ```

   to

   ```
   # 0.6.3 — user-visible changes
   ```

   The section under it already documents the release's content, opening with
   "A package namespace can resolve straight from a directory on disk" — that is
   the `path` source this release exists to ship. Do not add a fresh empty
   `# Unreleased` heading above it; the next unreleased change adds its own.

5. Leave a commit ready for the operator to push. The rheo release runs off a
   pull request into `main`, so the operator will need a PR whose **title is
   exactly `v0.6.3`** and which carries the `release` label:
   `.github/workflows/release.yml` publishes the five crates to crates.io, tags
   the merged commit by matching `v([0-9]+\.[0-9]+\.[0-9]+)`, and then uploads
   six platform zips with `tag_name: ${{ github.event.pull_request.title }}`. A
   PR titled anything else produces a release under a nonsense tag. Say this
   plainly in the report back, because the title is the release mechanism.

## Non-goals

- **Do not push, open the PR, label it, or merge it.** The operator does every
  outward-facing step. This bird ends with the bump prepared locally.
- **Do not touch the `rookery` repository.** Repinning its CI to the new
  release is a separate bird there, labelled `fix-rheo-path-floor` like this
  one, and it cannot start until the release has actually published.
- **Do not change any behaviour.** No code fixes, no doc rewrites beyond the
  changelog heading, no dependency bumps. A release commit that also changes
  behaviour is a release nobody can bisect.
- **Do not edit `flake.nix`.** It pins no rheo version string of its own.

## VERIFY

```bash
cd /home/lox/code/_fcl/rheo
cargo check --workspace                  # succeeds, and rewrites Cargo.lock
cargo run -p rheo -- --version           # must print exactly: rheo 0.6.3
rg -n '0\.6\.2' Cargo.toml               # must print nothing
rg -n '^version = "0\.6\.3"' Cargo.lock  # must print five lines
head -1 changelog.md                     # must print: # 0.6.3 — user-visible changes
```

Then confirm the new binary accepts the config key this release exists for, by
pointing it at a rookery demo:

```bash
cd /home/lox/code/_fcl/rheo && cargo build -p rheo
cd /home/lox/code/_fcl/rookery/core/0.1.0/demo/rheo \
  && /home/lox/code/_fcl/rheo/target/debug/rheo compile .
```

The log must include a line reading `@rookery resolves from … (a directory on
disk)`, and the compile must succeed. This reads the rookery checkout but
writes only into its gitignored `build/` directory, so it does not count as
touching that repository.
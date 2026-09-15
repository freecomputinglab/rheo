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
a rheo release. This bird prepares the release commit; the operator opens the
PR.

Touches: Cargo.toml, changelog.md (and `Cargo.lock` only if `cargo check` moves it)

## Do not start until the operator says so

**This bird runs AFTER `feat/vertebra-prelude` has been reviewed and merged into
`main`, as its own PR. The release is a second, separate PR on top.** The
operator merges; you never do.

Two checks before touching anything. If either fails, stop and report — do not
proceed, and do not "help" by merging or rebasing:

```bash
cd /home/lox/code/_fcl/rheo
jj log -r main --no-graph -T 'description.first_line() ++ "\n"'   # must NOT be "Support for incremental compile (#174)"
jj file show -r main Cargo.toml | rg -n '^version'                # must print 0.6.3
```

`main`'s tip being `#174` means the merge has not happened yet. The version
already reading `0.6.3` on `main` is the positive signal that the branch landed
(see "The bump is already half-done" below).

**The paired rheo-tests branch must merge first.** rheo's CI clones a
rheo-tests branch named `rheo/<this-branch>` when one exists and falls back to
`main` otherwise (`.github/workflows/ci.yml:32-42`, and the same logic in
`compat.yml:24-35`). A release PR's branch has no paired snapshot, so its CI
clones rheo-tests **`main`** — which, until `rheo/feat/vertebra-prelude` is
merged there, still carries the pre-rename marrow fixture and none of the
`auto_index` coverage. The release PR would go red through no fault of its own.
rheo-tests' own `README.md` states the order: merge the rheo-tests PR first.
Confirm with:

```bash
cd /home/lox/code/_fcl/rheo-tests
jj log -r main --no-graph -T 'description.first_line() ++ "\n"'   # must be the vertebra-prelude work, not the older tip
ls cases/marrow_names/content/                                    # must show .marrow.prologue.typ, not .marrow.prelude.typ
```

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

A developer machine hid this completely for a while, because a locally built
`rheo` still reported `rheo 0.6.2` — the binary that has `path` and the binary
that does not printed the same version string.

0.6.3 is no longer only the `path` fix. Once `feat/vertebra-prelude` merges,
the release also carries `[spine] auto_index` (default-on, and it changes what
every existing project builds), `[spine] prelude`, the `[marrow]` table and
`.marrow.prologue.typ` rename, `synthesized` on `spine`/`spine-flat`, and the
per-vertebra `rheo-index()` binding. `changelog.md`'s `# Unreleased` section
already documents all of it — that section, retitled, IS the release notes.

## The bump is already half-done

`feat/vertebra-prelude` bumped the workspace version to `0.6.3` as part of
`rh-rename-dot-marrow-is-epilogue-9b3bbd43`, not as a release act. It had to:
`rheo migrate`'s gate is `if from >= to { "already up to date" }`, so a fixture
pinned at `version = "0.6.2"` could never exercise a migration shipped by a
crate still calling itself `0.6.2`. **Leave that bump alone. Do not revert it
and do not re-apply it.**

What it did NOT do is move the four path-dependency constraints. MEASURED on
the branch:

- `Cargo.toml:6` — `version = "0.6.3"` (done)
- `Cargo.toml:29-32` — `rheo-core`/`rheo-html`/`rheo-pdf`/`rheo-epub` each still
  carry `version = "0.6.2"` (not done)
- `Cargo.lock:2629,2650,2686,2706,2727` — all five already read `0.6.3` (done,
  as a side effect of building the branch)

The stale constraints build fine, because `version = "0.6.2"` on a `0.x` crate
means `^0.6.2`, which a `0.6.3` sibling satisfies. They matter at publish time:
crates.io records the requirement as written, so the published `rheo 0.6.3`
would declare a floor of `rheo-core 0.6.2` it was never built against.

## Steps

1. Work on `main` after the merge (see the gate above). Start from a clean
   working copy with nothing else in it — a release commit carries the bump and
   the changelog heading, and nothing else at all.

2. In `/home/lox/code/_fcl/rheo/Cargo.toml`, change `0.6.2` to `0.6.3` on lines
   29-32 only:

   ```toml
   rheo-core = { path = "crates/core", version = "0.6.2" }   # line 29
   rheo-html = { path = "crates/html", version = "0.6.2" }   # line 30
   rheo-pdf = { path = "crates/pdf", version = "0.6.2" }     # line 31
   rheo-epub = { path = "crates/epub", version = "0.6.2" }   # line 32
   ```

   Line 6 is already `0.6.3`; do not touch it. Leave every other `0.6.2` in the
   repository alone — the ones under `.birds/` are fixture text inside a bird's
   own description, and the ones in `changelog.md` below the unreleased section
   are older releases' own headings and prose.

3. Confirm the lock still agrees:

   ```bash
   cd /home/lox/code/_fcl/rheo && cargo check --workspace
   ```

   Expect `Cargo.lock` NOT to change — all five entries already read `0.6.3`,
   and loosening a requirement from `^0.6.2` to `^0.6.3` does not move a
   resolved version. If the lock does change, read the diff and say what moved
   before continuing; a dependency bump riding along on a release commit is
   exactly what the non-goals forbid.

4. In `/home/lox/code/_fcl/rheo/changelog.md`, retitle line 1 from

   ```
   # Unreleased — user-visible changes
   ```

   to

   ```
   # 0.6.3 — user-visible changes
   ```

   Do not add a fresh empty `# Unreleased` heading above it; the next
   unreleased change adds its own. Do not rewrite the sections under it — they
   were written by the birds that landed each change, and this bird documents
   nothing new.

5. Leave one commit ready for the operator to push, and stop. Tell the operator
   in the report, plainly, that the PR **title must be exactly `v0.6.3`** and it
   must carry the `release` label: `.github/workflows/release.yml` publishes the
   five crates to crates.io, tags the merged commit by matching
   `v([0-9]+\.[0-9]+\.[0-9]+)`, and uploads six platform zips with
   `tag_name: ${{ github.event.pull_request.title }}`. A PR titled anything else
   produces a release under a nonsense tag. The title is the release mechanism,
   so it is worth a sentence of its own in the report.

## Non-goals

- **Do not push, open the PR, label it, or merge it** — and do not merge
  `feat/vertebra-prelude` either. Every outward-facing step is the operator's.
  This bird ends with the commit prepared locally.
- **Do not revert or re-do the workspace version bump.** It is already `0.6.3`
  and it belongs to the marrow-rename bird.
- **Do not touch the `rookery` repository.** Repinning its CI to the new
  release is a separate bird there, labelled `fix-rheo-path-floor` like this
  one, and it cannot start until the release has actually published.
- **Do not change any behaviour.** No code fixes, no doc rewrites beyond the
  changelog heading, no dependency bumps. A release commit that also changes
  behaviour is a release nobody can bisect.
- **Do not edit `flake.nix`.** It pins no rheo version string of its own.
- **Do not bless or regenerate anything in `../rheo-tests`.** If its suite is
  red against this commit, that is a finding to report, not a reference to
  rewrite on the way to a release.

## VERIFY

```bash
cd /home/lox/code/_fcl/rheo
cargo check --workspace                  # succeeds; Cargo.lock unchanged
cargo run -p rheo -- --version           # must print exactly: rheo 0.6.3
rg -n '0\.6\.2' Cargo.toml               # must print nothing
rg -n '^version = "0\.6\.3"' Cargo.lock  # must print five lines
head -1 changelog.md                     # must print: # 0.6.3 — user-visible changes
cargo test && cargo clippy --all-targets -- -D warnings   # both clean
```

Then the integration suite, which is what the release PR's CI will run, against
rheo-tests `main` (not a paired branch — a release branch has none):

```bash
cd /home/lox/code/_fcl/rheo-tests \
  && RHEO_MANIFEST=../rheo/Cargo.toml cargo test --test harness
```

It must be fully green. It stood at 145 passed, 0 failed when this bird was
written.

Finally confirm the new binary accepts the config key this release exists for,
by pointing it at a rookery demo:

```bash
cd /home/lox/code/_fcl/rheo && cargo build -p rheo
cd /home/lox/code/_fcl/rookery/core/0.1.0/demo/rheo \
  && /home/lox/code/_fcl/rheo/target/debug/rheo compile .
```

The log must include a line reading `@rookery resolves from … (a directory on
disk)`, and the compile must succeed. This reads the rookery checkout but
writes only into its gitignored `build/` directory, so it does not count as
touching that repository.

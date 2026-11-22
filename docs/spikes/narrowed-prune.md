# Spike: pruning minted pages on a narrowed rebuild

**Verdict: yes. A narrowed rebuild can prune the pages a recompiled vertebra
stopped minting, if each minted page names its owner when it is minted. rheo
cannot work the owner out for itself, so the page has to declare it. A
prototype with three parts removed every ghost-page symptom and left full
builds byte-identical:**

1. **`rheo-document(..., owner: <handle>)`.** This is a new optional argument.
   When it is set, the minted document carries
   `#metadata((path: path, owner: owner)) <rheo-mint:owner>`, and that metadata
   renders nothing.
2. **rheo harvests the owners after every compile.** It keeps a map of
   path → owning vertebra for each plugin, next to `written_pages`. At the end
   of a narrowed rebuild it deletes every page that is **owned by a vertebra
   this rebuild recompiled and that no round of this rebuild emitted**. Each
   narrowed round also adds the pages it wrote to `written_pages`, so the next
   full prune knows about them.
3. **rookery passes `owner: rec.origin`** on the note page it mints (one line
   in `@rookery/core`'s `.marrow.typ`).

The fix needs two birds. The rheo one comes first, because a rookery that
passes `owner:` fails against a rheo whose `rheo-document` has no such
parameter ("unexpected argument").

## Measurement conditions

- rheo baseline: this workspace at `c1372109` ("Gives handle anchors a literal
  HTML id"), built as a debug binary into its own `CARGO_TARGET_DIR`. The
  prototype was a `/tmp` copy of the same tree with the changes below, built
  the same way. Both binaries were built inside this workspace's devShell. A
  `/tmp` copy has no `.git`, so its flake will not evaluate.
- rookery: `/home/lox/code/_fcl/rookery` at `0fcb5f59` ("Lists every idea in a
  narrowed ideas-outline"), working copy included. The baseline used a plain
  `/tmp` copy of it. The prototype used a second copy with the one-line
  `owner:` change.
- rheo-tests: run as
  `ROOKERY_DIR=<rookery copy> RHEO_BIN=<binary> TYPST_IGNORE_SYSTEM_FONTS=1 cargo test --no-fail-fast --test watch_narrow_rookery_notes --test watch_narrow_rookery_structure -- --ignored rename_note_id note_lifecycle add_and_remove_pages`.
  `RHEO_BIN` rather than `RHEO_MANIFEST` because the harness's `cargo run`
  blocked on a cargo lock for longer than its 30s server-URL timeout.
- waterline: a `/tmp` copy of `waterline/rookery/` with the build dir excluded
  and `[packages.rookery]` changed to `path = <rookery copy>`. The run was
  `rheo watch <copy> --html --input today=2026-11-16` with `RUST_LOG=info`.
  After the first full build, the driver deleted `#idea(title: [Zoom room])[…]`
  from `nest/tidbits.typ` and waited for the `narrowed:` line. That edit
  narrows to 7 vertebrae in 2 rounds.
- Another agent was running rheo-tests on the machine at the same time, so no
  timing below means anything.

## How a minted page reaches rheo today

`@rookery/core`'s `.marrow.typ` loops over `_registry-final()` and calls
`rheo-document(page-at.file, handle: page-at.handle, ...)` once per note. That
registry is the *live* registry, which holds only the notes of vertebrae that
compiled in this pass. ("Minting stays on the live `registry`" is the file's own
comment. Relations read the merged `_reg-final()`.) After the loop it mints
`ideas/index.html` from the *merged* registry. Rust-side,
`flatten_bundle_outputs` marks every document that no spine vertebra owns as
`contributed: true`, and that flag is all rheo knows about a minted page.

Pruning happens only in `Build::run`. That function compares
`written_pages[plugin]` from the previous full run with this run's outputs
(`prune_stale_pages`). `narrow_one_plugin` and `write_subset_outputs` never
touch `written_pages`. That one gap produces all three rheo-tests symptoms:

- `rename_note_id` and `note_lifecycle`: the recompiled vertebra no longer
  mints `ideas/n09a.html` or `ideas/n07a.html`. Nothing in a narrowed rebuild
  deletes anything, so the old page stays.
- `add_and_remove_pages`: `ideas/n13a.html` was first minted by a *narrowed*
  rebuild, so it never entered `written_pages`. When page n13 is deleted, the
  full rebuild prunes against a set that never contained n13a.

## Q1. Which minted pages does a narrowed compile emit? Only the notes of compiled vertebrae, plus `ideas/index.html`

The prototype logged, at the end of the narrowed rebuild, every page it
emitted, grouped by declared owner:

```
SPIKE narrowed mint attribution emitted=107 owned_by_recompiled=99 owned_by_other=[] unowned_nonvertebra=["ideas/index.html"]
SPIKE owners before previously_owned=1592
SPIKE narrowed prune stale={"ideas/zoom-room.html"}
narrowed: 7 vertebra(e) in 2 round(s) — nest:tidbits, nest:jobs:cycle-25-26, nest:clusters:digitaltheory:review, nest:jobs, nest:jobs:cycle-26-27, index, nest:clusters:liminal:airborne-review-4sep26
```

107 = 7 vertebra pages + 99 note pages owned by those 7 vertebrae + 1
`ideas/index.html`. No emitted page was owned by a vertebra outside the
narrowed set. The other 1,493 owned note pages (1,592 − 99) were not emitted,
and they must not be pruned. Their absence from this compile says nothing about
whether they still exist. That is exactly why a set difference over the whole
contributed set (what `prune_stale_pages` does for full runs) cannot be used
here.

`ideas/index.html` is built from the merged registry, which includes fed data
from vertebrae the narrowed compile did not see. It has no owner and is
re-emitted by every compile. **A page like this must never be pruned by a
narrowed rebuild.** The rule below guarantees that, because it only ever
considers pages with a declared owner. Bundle `asset()` outputs (rookery's
search index and todos JSON, `@rheo/feeds`' feeds) are single files emitted on
every compile. They are outside this question and the prototype does not touch
them.

## Q2. Can rheo learn a contributed page's owner generically? Yes, but only if the page declares it

The three candidates the bird named:

- **`state("rheo-handle")` at the minted document: no.** `rheo-document` runs
  `rheo-page-init(handle)`, so inside the page the state is the page's *own*
  handle (`ideas:<slug>`). Before that it is the last spine vertebra's handle,
  which is stale (rookery's `.marrow.typ` measured this: "it emitted
  `index.html` from `ideas/rookery.html`, one level short"). Neither one is the
  note's owner. Marrow runs at bundle root, outside every vertebra, so no
  ambient context can supply the owner.
- **A reserved argument on `rheo-document`: yes (prototyped).** The declaration
  is made in the same call that mints the page, so the two cannot drift. A page
  that is minted always carries its claim, and a claim cannot exist without the
  page. The harvest is one introspector query for `<rheo-mint:owner>` metadata,
  shaped like `harvest_ligaments`. An owner that is not a known vertebra handle
  is ignored, which leaves the page unowned and so never pruned by a narrowed
  rebuild.
- **A generic ligament (`rheo-mint:<path>` attached on the owning page): would
  work, but worse.** rookery already attaches with `page: rec.origin`, so the
  harvest would key it by owner for free. However, the claim would then be
  separate from the minting call and could drift from it. It would also add a
  key to the ligament feed that every page's library hashes, and it would make
  rheo give meaning to one ligament key, when today rheo interprets none.

**Inference without a declaration is impossible.** A narrowed compile of
subset S emits only S's owned pages, as Q1 measured. A page that disappears
from the output could therefore have been deleted from S, or it could belong
to a vertebra outside S. Without an owner, nothing distinguishes the two cases.

## Q3. The pruning rule, and moving notes: yes, it is safe as long as the owner contract holds

Let R be the vertebrae recompiled across every round of the narrowed rebuild,
and E the pages emitted across every round. Let `owners` be the path → handle
map from the most recent harvest that wrote each page. Then:

1. prune = { p : owners[p] ∈ R and p ∉ E }
2. delete prune from disk (`prune_stale_pages`) and from `owners` and
   `written_pages`
3. fold this rebuild's harvested owners into `owners`, last writer wins
4. add E to `written_pages` *every round*, not only at the fixpoint, so a
   `ThresholdExceeded` fall-back to a full run still prunes pages that an
   aborted narrowed round wrote

A full run replaces `owners[plugin]` outright with its own harvest, just as it
already replaces `written_pages`.

How a moved note behaves under this rule (reasoned, not prototyped):

- **A → B in one rebuild (both files in the change set):** B emits the page, so
  it is in E and is not pruned. Its owner becomes B.
- **Two saves, A first (delete from A):** the first rebuild prunes the page,
  and the second, from B, mints it again. The page is missing for one rebuild,
  which is correct for the state on disk at that moment.
- **Two saves, B first:** B mints the page and its owner becomes B. When A is
  then recompiled without the note, owners[p] = B ∉ R, so the page is kept.
  Correct.
- A note whose page is minted under one vertebra but whose owner names another
  that did not compile is never pruned, so the failure mode is a ghost, never a
  lost page.

**The contract this rests on**, which belongs in `docs/contract.md` next to
`rheo-document`: *`owner: h` promises that every compile that compiles `h`
emits this page for as long as it should exist.* A package that sets an owner
on a page minted from fed (merged) data violates the promise, and its page can
be pruned wrongly. rookery's note pages satisfy it: they come from the live
registry and are keyed by `rec.origin`. rookery's `ideas/index.html` does not
satisfy it and correctly declares no owner.

## Evidence

rheo-tests, baseline vs prototype:

| test | baseline | prototype |
|---|---|---|
| `rename_note_id` | FAIL: `ideas/n09a.html: extra in actual` at both steps | ok |
| `note_lifecycle` | FAIL: `ideas/n07a.html: extra in actual` after deleting n07a | ok |
| `add_and_remove_pages` | FAIL: `ideas/n13a.html: extra in actual` on "delete page n13" (diffs=2); `notes.html` on the two narrowed steps | FAIL: `notes.html` only, on "create page n13" and "edit page n13 body"; "delete page n13" now diffs=0 |

The `notes.html` difference that remains is the auto-index symptom tracked by
`wl-refresh-auto-index-pages-on-narrowed-f075b666`. It is not a pruning
problem.

waterline, after deleting the Zoom room idea and rebuilding narrowed:

- baseline: `build/html/ideas/zoom-room.html` still on disk; 2,395 `.html` files.
- prototype: `zoom-room.html` gone; 2,394 `.html` files. The rest of the file
  list is identical to the baseline's.

Full builds byte-identical (`diff -r` over the whole build dir; baseline rheo
with baseline rookery vs prototype rheo with prototype rookery):

- rheo-tests `cases/watch_narrow_rookery` (`rheo compile --html`, 36 pages):
  identical.
- waterline `rheo compile --html --input today=2026-11-16` (2,395 pages,
  35.0 MB): identical.

The `<rheo-mint:owner>` metadata therefore costs nothing in output, and it
does not move any `loc-N` anchor.

## What the prototype did not do (the implementation must)

- **Tell the dev server.** `rheo watch --open` serves from an in-memory
  `VirtualFs` and forgets only the paths that each narrowed round passes to
  `on_round` (`crates/cli/src/lib.rs`, `server.forget_virtual_paths`). Pruned
  paths have to go through the same call, or the server keeps serving the
  deleted page from memory. The prototype added them only to
  `narrowed_outputs`, which the CLI never reads for this.
- Tests and docs: unit tests in `build.rs` beside the `rebuild_` tests, the
  `owner:` argument in `docs/link-rule.md`'s `rheo-document` section, and the
  contract in `docs/contract.md`.
- Persistence. `owners`, like `written_pages`, lives in memory for one
  `Build`. A new `watch` session starts with a full build that has no previous
  set, and one-shot `rheo compile` still prunes nothing. Both limits were
  already there from `rh-prune-output-of-deleted-vertebrae-59325112`, and
  waterline's `just build` wipes the build dir anyway.

## Q4. Recommendation

Implement the three-part design above, in two birds:

1. **rheo** (`/home/lox/code/_fcl/rheo`): `rheo-document(owner:)`, the
   `<rheo-mint:owner>` harvest, the narrowed prune and `written_pages` update,
   dev-server forgetting, tests and docs.
2. **rookery** (`/home/lox/code/_fcl/rookery`), blocked on 1: pass
   `owner: rec.at("origin", default: none)` on the note page in
   `core/0.1.1/.marrow.typ`. Leave `ideas/index.html` unowned. Then un-ignore
   or re-reason the three rheo-tests above.

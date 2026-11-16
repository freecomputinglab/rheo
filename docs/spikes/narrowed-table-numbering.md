# Spike: why a narrowed rebuild numbers tables differently from a full compile

**Verdict: the cause is the shared bundle counter, and the full build is the
one that is wrong.** rheo compiles a project as one Typst bundle, with one
`document(...)` per output page and a single introspector shared by all of
them. Typst's figure counter for each kind (`counter(figure.where(kind:
table))`, likewise `image` and `raw`) runs across every page in bundle order,
and so does `counter(math.equation)`. A narrowed compile leaves some pages out,
so fewer tables come before each page and its numbers drop. rheo already resets
the footnote counter per page in `rheo-page-init` (`crates/core/src/typ/rheo.typ`).
It resets no other counter. **Owner: rheo.** The fix is to reset the
built-in figure and equation counters next to the footnote reset. One bird
filed: `wl-number-figures-per-page-in-per-page-4e479d49`.

Neither index page computes anything from tables. `nest/dissertation.html` is
an `#index` of tagged chapter pages, and `chapters/c5.html` is a list of
`#window(name)` transclusions. Both pull in whole sections, and the tables come
in with them. A table's caption number is whatever the shared counter holds at
that point in the bundle.

## Measurement conditions

- rheo: rheo main's release binary at `/tmp/main-target/release/rheo` (the
  `rheo-b645` workspace's parent, `825a56a9`, "Moves the spine behind a
  constant-hash store"). The fix probe was a `--release` build of a `/tmp`
  copy of the same tree with only `rheo.typ` patched, in its own target dir.
- waterline: a `/tmp` copy of `rookery/` (build directory excluded), with
  `[packages.rookery]` replaced by `path = "/home/lox/code/_fcl/rookery"`.
- Narrowed: `rheo watch rookery --input today=2026-11-15 --html --trigger stdin
  --events <f>`, driven through a fifo. After the first `build_finished`, I
  appended one sentence to
  `nest/dissertation/chapters/c0/2.that-which-we-call-a-rose.typ`, sent
  `rebuild nest/dissertation/chapters/c0/2.that-which-we-call-a-rose.typ`, and
  waited for the second `build_finished` (`"kind":"narrowed"`). The rebuild
  narrowed to five vertebrae: `c0:2_that-which-we-call-a-rose`, `c0`,
  `c5:6_value-is-an-automatic-subject`, `dissertation`, `c5`.
- Full: `rheo compile rookery --html --input today=2026-11-15` on the same
  source, run from the copy's root.
- Numbers: `grep -oP 'figcaption>Table.\K[0-9]+'`. Typst puts a non-breaking
  space between the supplement and the number, so a literal `Table [0-9]`
  pattern matches nothing.

## Evidence

### 1. The reproduction

`diff -rq narrowed full` lists exactly two files:

| page | narrowed | full |
|---|---|---|
| `nest/dissertation.html` | 1 … 11 | 3 … 13 |
| `nest/dissertation/chapters/c5.html` | 12 … 16 | 14 … 18 |

I replaced each caption's number with a literal `N` in both builds
(`sed -E 's/(figcaption>Table.)[0-9]+/\1N/g'`), and the pages came out
byte-identical. The caption numbers
are the only difference. In the narrowed bundle nothing comes before
`dissertation`, so it starts at 1, and `c5` continues from 12. The full bundle
has two numbered table figures from vertebrae that come before `dissertation`
and are outside the narrowed set.

### 2. The full build's numbering is itself wrong

In a full build, the same table gets a different number depending on which page
shows it:

| page | table captions (full build, rheo main) |
|---|---|
| `nest/dissertation.html` | 3 … 13 |
| `nest/dissertation/chapters/c5.html` | 14 … 18 |
| `nest/dissertation/chapters/c5/3_cashing-down-and-covering-up.html` | 19 20 21 |
| `nest/dissertation/chapters/c5/5_money-is-a-real-abstraction.html` | 22 |
| `nest/dissertation/chapters/c5/8_causing-trouble-with-kant.html` | 23 |
| `nest/dissertation/chapters/c5/9_monsieur-le-capital.html` | 24 … 29 |
| `ideas/cashing-down-and-covering-up.html` | 33 34 35 |
| `ideas/money-is-a-real-abstraction.html` | 36 |
| `ideas/causing-trouble-with-kant.html` | 37 |
| `ideas/monsieur-le-capital-and-madame-la-terre.html` | 38 … 43 |

"The capitalist contradiction" is Table 19 on its own section page and Table 33
on its minted idea page. These are standalone HTML pages, and the reader has no
way to see bundle order. Page-local numbering is the intended reading. It is
the same reasoning that made rheo reset footnotes per page.

### 3. Removal probe in the project (prelude)

I added the following to the end of the copy's `_lib/prelude.typ`, which is
prepended inside every vertebra:

```typ
#counter(figure.where(kind: table)).update(0)
#counter(figure.where(kind: image)).update(0)
```

After that, narrowed and full matched (`diff -rq` printed nothing), and every
vertebra numbered from 1 (`dissertation.html` 1 … 11, `c5.html` 1 … 5,
`c5/3_…` 1 2 3). The idea pages still accumulated (`ideas/cashing-…` 4 5 6,
`ideas/monsieur-…` 9 … 14). rookery mints those pages from marrow, and the
vertebra prelude never runs there.

### 4. Fix probe in rheo (`rheo-page-init`)

I reverted the prelude and patched only `rheo.typ` in a `/tmp` copy of rheo. Inside the existing
`if per-page and reset { … }` block, next to `counter(footnote).update(0)`, I added:

```typ
counter(figure.where(kind: table)).update(0)
counter(figure.where(kind: image)).update(0)
counter(figure.where(kind: raw)).update(0)
counter(math.equation).update(0)
```

I rebuilt and repeated the same narrowed and full runs:

- `diff -rq narrowed-fix full-fix` printed nothing.
- Every page with tables now starts at 1, idea pages included:
  `ideas/cashing-down-and-covering-up.html` 1 2 3,
  `ideas/monsieur-le-capital-and-madame-la-terre.html` 1 … 6. rookery mints
  through `rheo-document`, which calls `rheo-page-init`, so one reset covers
  vertebrae and minted pages alike.
- Between full builds before and after the patch, 32 pages changed. On 30 of
  them, the table/figure/listing caption numbers were the only change. The other
  two (`ideas/old-intro.html`, `nest/papers/marx-freud-isomorphism.html`) show
  only a narrower `html.frame` SVG. That frame holds a numbered `figure` (a
  cetz canvas), so the caption number is drawn into the SVG and takes fewer
  digits. It is the same change in rendered form.

## Where the fix belongs, and why not a ligament

A `paper-words`-style ligament would carry the full build's number into the
narrowed compile. That would make narrowed match full, but it would also keep
a number that Section 2 shows is wrong. The ligament route fits when the full
build's cross-page value is the intended one, as with a word total summed over
sections. Here it is not. Resetting per page makes the number a property of the
page alone, so narrowed and full agree by construction and no dependency edge
is needed.

The reset belongs in rheo rather than rookery or waterline for three reasons:

- The shared counter is a consequence of rheo's bundle, not of any template.
- rheo already owns the matching footnote reset and its gate. The
  `reset_footnotes` toggle only acts on per-page formats, so the combined PDF
  stays continuous.
- `rheo-page-init` is the one hook that runs for both vertebrae and
  `rheo-document`-minted pages. A project-level fix (Section 3) misses minted
  pages.

The fix has limits. Figures with a custom `kind:` are not reset, because their
kinds are not knowable in `rheo-page-init`. The fix reuses the
`reset-footnotes` gate instead of adding a key. If a project ever wants
continuous figures with per-page footnotes, that can be split later.

## Birds filed

- `wl-number-figures-per-page-in-per-page-4e479d49`: **Number figures per
  page in per-page output** (rheo, `incremental-compile`). It resets the table,
  image and raw figure counters and the equation counter in `rheo-page-init`
  under the existing gate, and documents the extended toggle. Its VERIFY
  reproduces the 1 … 11 vs 3 … 13 difference with rheo main and shows
  `diff -rq` empty with the fix.

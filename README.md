<p align="center">
  <img src="./header.svg" alt="Project header" width="600px">
</p>

Rheo is a typesetting and static site engine based on [Typst](https://typst.app/).
It compiles folders of Typst to PDF, HTML, and EPUB simultaneously, and ships a
development server for rapid website iteration. Rheo is a project of the
[Free Computing Lab](https://freecomputinglab.ohrg.org) and part of the document
infrastructure described in [Document Infrastructure for Augmented Reading](https://chi-star-workshop.github.io/src/assets/pdf/papers/DocumentInfrastructureForAugmentedReading%20-%20Will%20Crichton.pdf).

Full documentation lives at **[rheo.ohrg.org](https://rheo.ohrg.org)**.

## Usage

Compile every `.typ` file in a directory to all formats, recompiling on change:

```bash
# Clone the examples repo first
git clone https://github.com/freecomputinglab/rheo-tests.git ../rheo-tests
rheo watch ../rheo-tests/examples/blog_site --open
```

`--open` starts a development server at `http://localhost:3000` with automatic
browser refresh. Use `compile` for a one-shot build, and `--pdf` / `--html` /
`--epub` to select formats:

```bash
rheo compile ../rheo-tests/examples/blog_site --html
```

See the [documentation](https://rheo.ohrg.org) for the full set of commands,
flags, and `rheo.toml` configuration.

## Installation

### Using cargo binstall (recommended)

[cargo-binstall](https://github.com/cargo-bins/cargo-binstall) downloads a
prebuilt binary from GitHub Releases — no compiling from source:

```bash
cargo binstall rheo
```

### Using cargo

Rheo requires Rust and Cargo (install from [rustup.rs](https://rustup.rs/)):

```bash
# Install from crates.io
cargo install --locked rheo

# Or build from source
git clone https://github.com/freecomputinglab/rheo
cd rheo
cargo install --path .
```

### Using Nix flakes

With [Nix](https://nixos.org/download/) and
[flakes](https://nixos.wiki/wiki/Flakes) enabled:

```bash
nix develop   # enter the development environment
nix build     # or build the package
```

## Features

- **Multi-format compilation** — one source tree to PDF, HTML, and EPUB at once.
- **Relative linking** — link between documents with Typst label syntax; rheo
  resolves each link per output format (see below).
- **Spines** — combine and order files into a single PDF or multi-chapter EPUB.
- **Watch mode + dev server** — live reload at `http://localhost:3000`.
- **`rheo.toml` configuration** — formats, spines, assets, fonts, and per-format
  options.

See the [documentation](https://rheo.ohrg.org) for details on every feature.

### Relative linking

Rheo assigns each source file a *handle* derived from its path, and you link
between documents using standard Typst label syntax:

```typst
See the #link(<about>)[about page] for more information.
Visit #link("https://example.com")[our website].
```

Rheo resolves these per format — to `.html` anchors in HTML, and to internal
document links in PDF and EPUB. A link to a non-existent file is a compile error.

Root-level files get a bare handle (`<about>`); nested files use `:` as the path
separator (`<chapters:intro>`). See the
[relative linking docs](https://rheo.ohrg.org) for handle rules and migrating
projects from Rheo version `<0.4.0`.

## License

Licensed at your option under either:
- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE))
- MIT license ([LICENSE-MIT](./LICENSE-MIT))

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

<p align="center">
  <img src="./header.svg" alt="Project header" width="600px">
</p>

Rheo is a typesetting and static site engine based on [Typst](https://typst.app/).
You can use it to compile folders containing Typst to PDF, HTML, and EPUB simultaneously.
Rheo is a standalone CLI tool that includes a development server for rapid website iteration.

## Usage
Compile all `.typ` files in a directory to PDF, HTML, and EPUB and recompile on change:

```bash
rheo watch examples/blog_site --open
```

The `--open` flag starts a development server at `http://localhost:3000` with automatic browser refresh.

Use additional flags for customization:

```bash
# Custom config file location
rheo compile examples/blog_site --config /path/to/custom.toml

# Custom build directory
rheo compile examples/blog_site --build-dir /tmp/build
```

See [the documentation](https://rheo.ohrg.org) for more information regarding which flags are available.

## Installation
### Using cargo (Recommended)
Rheo requires Rust and Cargo.
Install from [rustup.rs](https://rustup.rs/).

```bash
# Install from crates.io
cargo install rheo

# Or build the project from source
git clone https://github.com/freecomputinglab/rheo
cd rheo
cargo build --release
cargo install --path .
```
### Using Nix flakes
To install the compilation environment, first ensure that you have [Nix](https://nixos.org/download/) installed on your computer.
You will also need to enable [Nix flakes](https://nixos.wiki/wiki/Flakes), which you can do by inserting the following line in your `nix.conf`:

```conf
experimental-features = nix-command flakes
```

With Nix and flakes installed, run:

```bash
# Enter development environment
nix develop

# Or build the package
nix build
```

## Features
### Relative linking
Rheo automatically transforms cross-document links based on the output format to ensure they work correctly in each context.


```typst
See the #link("./about.typ")[about page] for more information.
Visit #link("https://example.com")[our website].
```

In HTML, this would compile as:
```html
See the <a href="./about.html">about page</a> for more information. Visit <a href="https://example.com">our website</a>.`
```

If a linked file doesn't exist, rheo will report a detailed error during compilation.

See [the documentation](https://rheo.ohrg.org) for more information.
### Multi-Format Compilation
Rheo compiles Typst documents to three output formats simultaneously:

- **PDF**: High-quality print-ready documents.
- **HTML**: Web-ready output with CSS customization.
- **EPUB**: E-book format with support for merged multi-chapter books.

By default, all three formats are generated. Use format flags to compile specific outputs:

```bash
rheo compile my_project --pdf       # PDF only
rheo compile my_project --html      # HTML only
rheo compile my_project --epub      # EPUB only
rheo compile my_project --pdf --html  # PDF and HTML
```

### Watch Mode
Watch mode automatically recompiles files when they change, perfect for iterative development:

```bash
rheo watch examples/blog_site
```

Add the `--open` flag to launch a development server with automatic browser refresh:

```bash
rheo watch examples/blog_site --open
```

The development server:
- Runs at `http://localhost:3000`
- Opens your default browser automatically
- Refreshes the page when files change
- Supports multiple connected browsers

### PDF and EPUB merging via spines
Combine multiple Typst files into a single PDF or EPUB document using the merge feature. Configure in `rheo.toml`:

```toml
[pdf.spine]
title = "My Book"
vertebrae = ["cover.typ", "chapters/**/*.typ"]
merge = true

[epub.spine]
title = "My Book"
vertebrae = ["cover.typ", "chapters/**/*.typ"]
```

The `vertebrae` uses glob patterns to specify which files to include and in what order.
Globbed files use lexicographic sorting.
### Automatic Defaults
Rheo automatically infers sensible defaults for EPUB:

- `title`: Derived from filename (single file) or directory name (project)
- `vertebrae`: Single file (single-file mode) or all `.typ` files sorted alphabetically (directory mode)

This means you can generate EPUBs without any configuration:

```bash
rheo compile my_document.typ --epub
# Creates my_document.epub with title "My Document"

rheo compile my_book/ --epub
# Creates my_book.epub with all .typ files included
```
### TOML Configuration
Projects can include a `rheo.toml` configuration file in the project root to customize compilation behavior rather than specifying flags.
See [the documentation](https://rheo.ohrg.org) for more information.

### CSS Styling
By default, rheo uses a simple, elegant and modern stylesheet to style your HTML.
To customize this, you can add a `style.css` in your project root, which rheo will inject into your HTML output.

## License
Licensed at your option under either:
- Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE))
- MIT license ([LICENSE-MIT](./LICENSE-MIT))
## Contribution
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

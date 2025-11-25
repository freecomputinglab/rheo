# Blog Site Example

This example demonstrates a typical blog structure with format-specific compilation.

## Directory Structure

```
blog_site/
├── rheo.toml              # Configuration file
└── content/               # Blog content
    ├── index.typ          # Landing page (HTML only)
    ├── severance-ep-1.typ # Blog post (both formats)
    ├── severance-ep-2.typ # Blog post (both formats)
    ├── severance-ep-3.typ # Blog post (both formats)
    ├── writing-in-typst.typ # Blog post (both formats)
    └── img/               # Images
```

## Configuration (rheo.toml)

```toml
content_dir = "content"

[html]
static_files = ["img/**"]

[pdf]
exclude = ["index.typ"]
```

**Note:** All patterns (`static_files` and `exclude`) are relative to `content_dir`, not the project root. This means `exclude = ["index.typ"]` matches `content/index.typ`.

### Format-Specific Filtering

This example demonstrates per-format exclusions:

- **index.typ**: Excluded from PDF compilation
  - This is a landing page meant for web viewing
  - PDF compilation is unnecessary for this file

- **Blog posts**: Compiled to both PDF and HTML
  - Individual blog posts can be read on the web or downloaded as PDFs
  - No exclusions, so they compile to all requested formats

## Use Case

This configuration is ideal for blogs or documentation sites where:
- The landing page/index should only be in HTML
- Content pages should be available in multiple formats

## Compiling

Compile all formats (HTML only for index, both formats for posts):
```bash
rheo compile examples/blog_site
```

Compile HTML only (all files):
```bash
rheo compile examples/blog_site --html
```

Compile PDF only (skips index.typ):
```bash
rheo compile examples/blog_site --pdf
```

## Output

Output will be in `build/blog_site/`:
- `html/` - Contains all HTML files and copied images
  - `index.html` (from index.typ)
  - `severance-ep-1.html`, `severance-ep-2.html`, `severance-ep-3.html`
  - `writing-in-typst.html`
  - `img/` directory
- `pdf/` - Contains PDF files for blog posts only
  - `severance-ep-1.pdf`, `severance-ep-2.pdf`, `severance-ep-3.pdf`
  - `writing-in-typst.pdf`
  - (no `index.pdf` - excluded by `pdf.exclude` pattern)

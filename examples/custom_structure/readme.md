# Custom Directory Structure Example

This example demonstrates rheo's ability to work with custom directory structures using `content_dir` and `static_files` configuration.

## Directory Structure

```
custom_structure/
├── rheo.toml              # Configuration file
├── content/               # Source .typ files (configured via content_dir)
│   ├── index.typ
│   ├── lib/
│   │   └── utils.typ      # Library file (excluded from compilation)
│   └── posts/
│       └── feature-overview.typ
├── static/                # Static assets
│   ├── css/
│   │   └── style.css
│   └── images/
│       └── diagram.svg
└── data/
    └── config.json
```

## Configuration (rheo.toml)

### content_dir
```toml
[compile]
content_dir = "content"
```

Only searches for .typ files within the `content/` directory instead of the entire project root.

### exclude patterns
```toml
[compile]
exclude = ["content/lib/**/*.typ", "content/_*.typ"]
```

Excludes library/template files from compilation.

### static_files
```toml
[html]
static_files = [
  "static/css/**",
  "static/images/**",
  "data/*.json"
]
```

Copies matching files to HTML output directory using glob patterns.

## Features Demonstrated

1. **Cross-directory imports**: Files in `content/posts/` can import from `content/lib/` using relative paths
2. **Compilation root**: The compilation root is set to `content/`, allowing proper path resolution
3. **Static file copying**: CSS, images, and data files are copied to output using glob patterns
4. **Exclusion patterns**: Library files in `content/lib/` are excluded from compilation

## Compiling

```bash
rheo compile . --html
```

Output will be in `build/custom_structure/html/` with:
- `index.html` and `feature-overview.html`
- `static/` directory with CSS and images
- `data/config.json`

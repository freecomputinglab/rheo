#import "../lib/utils.typ": highlight, chapter_header

#chapter_header[Feature Overview]

This page demonstrates cross-directory imports and asset references.

== Importing from Parent Directory

The `chapter_header` and `highlight` functions are imported from `../lib/utils.typ`, which is in a parent directory. This works because rheo sets the compilation root to `content_dir`.

== Static Assets

Custom CSS styles and images from `static/css/` and `static/images/` are copied to the HTML output directory using the glob patterns defined in `rheo.toml`.

Note: Static assets outside the `content_dir` cannot be referenced in Typst source files during compilation, but they are copied to the output for use in the final HTML.

== Data Files

Configuration data from `data/config.json` is also copied to the output directory thanks to the glob pattern `data/*.json` in `rheo.toml`.

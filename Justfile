# Default recipe: build all files in src folder
default:
    just build src

# Convert a single file to PDF, and HTML
convert FILE PROJECT="":
    just convert-pdf "{{FILE}}" "{{PROJECT}}"
    just convert-html "{{FILE}}" "{{PROJECT}}"

# Convert Typst file to PDF
convert-pdf FILE PROJECT="":
    #!/usr/bin/env fish
    set OUTDIR (test -n "{{PROJECT}}" && echo "build/{{PROJECT}}/pdf" || echo "build/pdf")
    mkdir -p "$OUTDIR"
    set BASENAME (basename "{{FILE}}" .typ)
    typst compile --root . --features html "{{FILE}}" "$OUTDIR/$BASENAME.pdf"

# Convert Typst file to HTML and EPUB
convert-html FILE PROJECT="":
    #!/usr/bin/env fish
    set OUTDIR (test -n "{{PROJECT}}" && echo "build/{{PROJECT}}/html" || echo "build/html")
    set OUTDIR_EPUB (test -n "{{PROJECT}}" && echo "build/{{PROJECT}}/epub" || echo "build/epub")
    mkdir -p "$OUTDIR"
    mkdir -p "$OUTDIR_EPUB"
    set BASENAME (basename "{{FILE}}" .typ)
    set FILEDIR (dirname "{{FILE}}")
    typst compile --root . --features html --format html "{{FILE}}" "$OUTDIR/$BASENAME.html"
    # Copy style.css from file's directory, or root if not found
    if test -f "$FILEDIR/style.css"
        cp "$FILEDIR/style.css" "$OUTDIR/style.css"
    else
        cp src/typst/style.css "$OUTDIR/style.css"
    end
    # Copy img directory from file's directory if it exists
    if test -d "$FILEDIR/img"
        cp -r "$FILEDIR/img" "$OUTDIR/"
    end
    # Convert to EPUB with styles
    ebook-convert "$OUTDIR/$BASENAME.html" "$OUTDIR_EPUB/$BASENAME.epub"

# Convert all files in a folder (default: src)
# Usage: just convert-all [FOLDER] [PROJECT]
# Examples: just convert-all
#           just convert-all examples/phd_thesis phd_thesis
convert-all FOLDER="src" PROJECT="":
    #!/usr/bin/env fish
    for file in {{FOLDER}}/*.typ
        if test -f "$file"
            just convert "$file" "{{PROJECT}}"
        end
    end

# Build a project folder into its own output directory
# Usage: just build FOLDER_PATH
# Examples: just build src                → outputs to build/pdf/ and build/html/
#           just build examples/academic_book → outputs to build/academic_book/pdf/ and build/academic_book/html/
build FOLDER:
    #!/usr/bin/env fish
    set PROJECT (basename "{{FOLDER}}")
    just convert-all "{{FOLDER}}" "$PROJECT"

build-examples FOLDER="examples": 
    #!/usr/bin/env fish
    for subfolder in {{FOLDER}}/**
        if test -d "$subfolder"
            just build "$subfolder"
        end
    end

# Clean build directory while preserving .gitignore
clean:
    #!/usr/bin/env fish
    if test -d build
        find build -mindepth 1 ! -name '.gitignore' -delete
    end

update:
  nix flake update

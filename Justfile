# Default recipe: build all files in chapters folder
default:
    just build chapters

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
    typst compile --root . "{{FILE}}" "$OUTDIR/$BASENAME.pdf"

# Convert Typst file to HTML
convert-html FILE PROJECT="":
    #!/usr/bin/env fish
    set OUTDIR (test -n "{{PROJECT}}" && echo "build/{{PROJECT}}/html" || echo "build/html")
    mkdir -p "$OUTDIR"
    set BASENAME (basename "{{FILE}}" .typ)
    set FILEDIR (dirname "{{FILE}}")
    typst compile --root . --features html --format html "{{FILE}}" "$OUTDIR/$BASENAME.html"
    # Copy style.css from file's directory, or root if not found
    if test -f "$FILEDIR/style.css"
        cp "$FILEDIR/style.css" "$OUTDIR/style.css"
    else
        cp style.css "$OUTDIR/style.css"
    end
    # Copy img directory from file's directory if it exists
    if test -d "$FILEDIR/img"
        cp -r "$FILEDIR/img" "$OUTDIR/"
    end

# Convert all files in a folder (default: chapters)
# Usage: just convert-all [FOLDER] [PROJECT]
# Examples: just convert-all
#           just convert-all examples/phd_thesis phd_thesis
convert-all FOLDER="chapters" PROJECT="":
    #!/usr/bin/env fish
    for file in {{FOLDER}}/*.typ
        if test -f "$file"
            just convert "$file" "{{PROJECT}}"
        end
    end

# Build a project folder into its own output directory
# Usage: just build FOLDER_PATH
# Examples: just build chapters           → outputs to build/pdf/ and build/html/
#           just build examples/academic_book → outputs to build/academic_book/pdf/ and build/academic_book/html/
build FOLDER:
    #!/usr/bin/env fish
    set PROJECT (basename "{{FOLDER}}")
    just convert-all "{{FOLDER}}" "$PROJECT"

# Clean build directory while preserving .gitignore
clean:
    #!/usr/bin/env fish
    if test -d build
        find build -mindepth 1 ! -name '.gitignore' -delete
    end

update:
  nix flake update

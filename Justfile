# Default recipe: convert all Orgmode files
default: convert-all

# Convert a single file to PDF, and HTML
convert FILE:
    just convert-pdf "{{FILE}}"
    just convert-html "{{FILE}}"

# Convert Typst file to PDF
convert-pdf FILE:
    #!/usr/bin/env fish
    mkdir -p build/pdf
    set BASENAME (basename "{{FILE}}" .typ)
    typst compile --root . "{{FILE}}" "build/pdf/$BASENAME.pdf"

# Convert Typst file to HTML
convert-html FILE:
    #!/usr/bin/env fish
    mkdir -p build/html
    set BASENAME (basename "{{FILE}}" .typ)
    typst compile --root . --features html --format html "{{FILE}}" "build/html/$BASENAME.html"
    cp style.css build/html/style.css

# Convert all files
convert-all FOLDER="chapters":
    #!/usr/bin/env fish
    for file in {{FOLDER}}/*.typ
        if test -f "$file"
            just convert "$file"
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

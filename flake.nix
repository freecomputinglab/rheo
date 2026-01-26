{
  description = "rheo - tool for flowing Typst documents into publishable outputs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    typst.url = "github:typst/typst-flake/main";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, typst, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        # Create crane library with default stable rust
        craneLib = crane.mkLib pkgs;

        # Source filtering to include Cargo files and resources in src/
        # Exclude .beads directory to avoid socket file issues
        src = pkgs.lib.cleanSourceWith {
          src = craneLib.path ./.;
          filter = path: type:
            let
              baseName = baseNameOf path;
            in
            (baseName != ".beads") &&
            ((craneLib.filterCargoSources path type) ||
            (pkgs.lib.hasInfix "/src/" path));
        };

        # Build *just* the cargo dependencies (for caching)
        cargoArtifacts = craneLib.buildDepsOnly {
          inherit src;
          buildInputs = with pkgs; [ openssl ];
          nativeBuildInputs = with pkgs; [ pkg-config perl ];
        };
      in
      {
        packages.default = craneLib.buildPackage {
          inherit cargoArtifacts src;

          buildInputs = with pkgs; [
            openssl
          ];

          nativeBuildInputs = with pkgs; [
            pkg-config
            perl
          ];
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
            pkg-config
            openssl

            # Temporary dev tools for comparison
            pandoc
            just
            # calibre # first example of ebook-convert command
            # fish # was needed for Justfile scripts in early phases, shouldn't be relevant now
          ] ++ [
            typst.packages.${system}.default
          ];

          shellHook = ''
            echo "rheo development environment loaded"
            echo "Run 'cargo build' to build the Rust binary"
          '';
        };
      });
}

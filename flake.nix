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
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Rust toolchain from rust-toolchain.toml (single source of truth)
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # Create crane library with custom rust toolchain
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
          nativeBuildInputs = with pkgs; [ pkg-config perl rustToolchain ];
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
            rustToolchain
          ];
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain from rust-overlay
            rustToolchain

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

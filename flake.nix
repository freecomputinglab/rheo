{
  description = "rheo - tool for flowing Typst documents into publishable outputs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    typst.url = "github:typst/typst/main";
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

        # Get Rust toolchain from rust-toolchain.toml
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # Create crane library with our custom toolchain
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # Custom source filter to include all files in src/ (typ, css, csl, etc.)
        srcFilter = path: _type: builtins.match ".*/src/.*" path != null;
        srcOrCargo = path: type:
          (srcFilter path type) || (craneLib.filterCargoSources path type);

        # Build *just* the cargo dependencies (for caching)
        cargoArtifacts = craneLib.buildDepsOnly {
          src = craneLib.path ./.; # Simplified src
          buildInputs = with pkgs; [ openssl ];
          nativeBuildInputs = with pkgs; [ pkg-config perl ];
        };
      in
      {
        packages.default = craneLib.buildPackage {
          inherit cargoArtifacts;
          src = craneLib.path ./.;

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
            rustToolchain
            pkg-config

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

{
  description = "rheo - tool for flowing Typst documents into publishable outputs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    typst.url = "github:typst/typst/main";
    rust-overlay.url = "github:oxalica/rust-overlay";
    naersk.url = "github:nix-community/naersk";
  };

  outputs = { self, nixpkgs, flake-utils, typst, rust-overlay, naersk }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Get Rust toolchain from rust-toolchain.toml
        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        # Create naersk builder with our Rust toolchain
        naersk' = pkgs.callPackage naersk {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
      in
      {
        packages.default = naersk'.buildPackage {
          src = ./.;

          # Build from src/rs directory
          buildInputs = with pkgs; [
            pkg-config
          ];

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain
            rustToolchain
            pkg-config

            # Typst and document tools
            pandoc
            just
            calibre # for ebook-convert command

            # Shell
            fish
          ] ++ [
            typst.packages.${system}.default
          ];

          shellHook = ''
            echo "rheo development environment loaded"
            echo "Run 'just' to compile all source files"
            echo "Run 'cargo build' to build the Rust binary"
          '';
        };
      });
}

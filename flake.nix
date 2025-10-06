{
  description = "rheo - tool for flowing Typst documents into publishable outputs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    typst.url = "github:typst/typst/main";
  };

  outputs = { self, nixpkgs, flake-utils, typst }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            pandoc
            fish
            just
          ] ++ [
            typst.packages.${system}.default
          ];

          shellHook = ''
            echo "rheo development environment loaded"
            echo "Run 'just' to compile all source files"
          '';
        };
      });
}

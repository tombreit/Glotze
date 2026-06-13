{
  description = "Glotze — search and download videos from public broadcaster Mediatheken (DACH region)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = pkgs.callPackage ./nix/package.nix { };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };

        # `nix develop` — a shell with the toolchain to build/hack Glotze.
        devShells.default = pkgs.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
        };
      }
    );
}

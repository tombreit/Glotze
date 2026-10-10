{
  description = "Glotze — search and download videos from public broadcaster Mediatheken (DACH region)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      # A GTK 4 desktop app: Linux only.
      forAllSystems = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
      ];
    in
    {
      # `nix run` uses this package's meta.mainProgram, so no `apps` output.
      packages = forAllSystems (system: {
        default = nixpkgs.legacyPackages.${system}.callPackage ./nix/package.nix { };
      });

      # `nix develop` — a shell with the toolchain to build/hack Glotze.
      devShells = forAllSystems (system: {
        default = nixpkgs.legacyPackages.${system}.mkShell {
          inputsFrom = [ self.packages.${system}.default ];
        };
      });
    };
}

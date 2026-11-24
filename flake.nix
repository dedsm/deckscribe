{
  description = "deckscribe: turn a recorded talk into its slides, the words spoken over each, and a description of each slide";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      overlays.default = final: _prev: { deckscribe = final.callPackage ./nix/package.nix { }; };

      packages = forAll (pkgs: rec {
        deckscribe = pkgs.callPackage ./nix/package.nix { };
        default = deckscribe;
      });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.deckscribe ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            ffmpeg-headless
            cargo-deny
            cargo-about
            cargo-cyclonedx
          ];
          LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux (
            pkgs.lib.makeLibraryPath [ pkgs.vulkan-loader ]
          );
        };
      });

      checks = forAll (pkgs: {
        # Builds the program and runs the workspace's tests.
        package = self.packages.${pkgs.stdenv.hostPlatform.system}.deckscribe;
        fmt = pkgs.runCommand "deckscribe-fmt" { nativeBuildInputs = [ pkgs.cargo pkgs.rustfmt ]; } ''
          cd ${self} && cargo fmt --all --check && touch $out
        '';
      });

      formatter = forAll (pkgs: pkgs.nixfmt);
    };
}

{
  description = "deckscribe: presentation video to slides, words and descriptions";
  # Same nixpkgs as POCs/flake.lock, so nothing new is downloaded.
  inputs.nixpkgs.url = "https://releases.nixos.org/nixos/unstable/nixos-26.11pre1074753.b1b875982b17/nixexprs.tar.xz";
  outputs = {nixpkgs, ...}: let
    pkgs = nixpkgs.legacyPackages.x86_64-linux;
    # transcribe-cpp-sys builds transcribe.cpp and ggml with CMake, and the
    # Vulkan backend compiles its shaders with glslc (shaderc), as in Hex's
    # nix/package.nix.
    nativeBuildInputs = with pkgs; [pkg-config cmake python3 shaderc];
    buildInputs = with pkgs; [vulkan-headers vulkan-loader spirv-headers];
    deckscribe = pkgs.rustPlatform.buildRustPackage {
      pname = "deckscribe";
      version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
      src = pkgs.lib.fileset.toSource {
        root = ./.;
        fileset = pkgs.lib.fileset.unions [./Cargo.toml ./Cargo.lock ./src];
      };
      cargoLock.lockFile = ./Cargo.lock;
      nativeBuildInputs = nativeBuildInputs ++ [pkgs.makeWrapper pkgs.autoAddDriverRunpath];
      inherit buildInputs;
      # CMake is for the -sys crate's build script, not for this package.
      dontUseCmakeConfigure = true;
      # The speech model is not part of the package: it is downloaded on first
      # run. ffmpeg is called as a program.
      postFixup = ''
        wrapProgram $out/bin/deckscribe --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.ffmpeg-headless]}
      '';
      meta.mainProgram = "deckscribe";
    };
  in {
    packages.x86_64-linux.default = deckscribe;
    devShells.x86_64-linux.default = pkgs.mkShell {
      packages = nativeBuildInputs ++ buildInputs ++ (with pkgs; [cargo rustc ffmpeg-headless]);
      LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [pkgs.vulkan-loader];
    };
  };
}

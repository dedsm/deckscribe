{
  lib,
  stdenv,
  rustPlatform,
  pkg-config,
  cmake,
  python3,
  shaderc,
  vulkan-headers,
  vulkan-loader,
  spirv-headers,
  makeWrapper,
  autoAddDriverRunpath,
  ffmpeg-headless,
}:

rustPlatform.buildRustPackage {
  pname = "deckscribe";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../skills
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;
  cargoBuildFlags = [ "-p" "deckscribe" ];
  cargoTestFlags = [ "--workspace" ];

  # transcribe-cpp-sys builds transcribe.cpp and ggml with CMake. On Linux the
  # Vulkan backend compiles its shaders with glslc (shaderc); macOS uses Metal.
  nativeBuildInputs = [
    pkg-config
    cmake
    python3
    makeWrapper
  ]
  ++ lib.optionals stdenv.hostPlatform.isLinux [
    shaderc
    autoAddDriverRunpath
  ];
  buildInputs = lib.optionals stdenv.hostPlatform.isLinux [
    vulkan-headers
    vulkan-loader
    spirv-headers
  ];
  # CMake is for the -sys crate's build script, not for this package.
  dontUseCmakeConfigure = true;

  # ffmpeg is called as a program. The speech model is not part of the
  # package: it is downloaded on first run.
  postFixup = ''
    wrapProgram $out/bin/deckscribe --prefix PATH : ${lib.makeBinPath [ ffmpeg-headless ]}
  '';

  meta = {
    description = "Turn a recorded talk into its slides, the words spoken over each, and a description of each slide";
    license = lib.licenses.mit;
    mainProgram = "deckscribe";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
    ];
  };
}

# THROWAWAY SPIKE: Zone's GAMES page as a native Rust + Slint binary.
#
#   nix build --impure --expr '(import <nixpkgs> { system = "aarch64-linux"; }).callPackage ./clients/zone-native-spike/package.nix {}'
#
# README.md, "Build", has the same command against this repo's pinned
# nixpkgs. Not wired into flake.nix.
{ pkgs }:
let
  lib = pkgs.lib;
  # winit, xkbcommon and fontique open these with dlopen at run time; none
  # is linked. They go on the binary's RUNPATH so the store path runs as it
  # is, under Wayland (sway) or X11. fontconfig is optional: the font is
  # embedded.
  runtimeLibraries = with pkgs; [
    wayland
    libxkbcommon
    libx11
    libxcursor
    libxrandr
    libxi
    libxcb
    fontconfig.lib
  ];
in
pkgs.rustPlatform.buildRustPackage {
  pname = "zone-native-spike";
  version = "0.0.0";

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./build.rs
      ./src
      ./ui
    ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  nativeBuildInputs = [ pkgs.patchelf ];

  postFixup = ''
    patchelf --add-rpath "${lib.makeLibraryPath runtimeLibraries}" "$out/bin/zone-native-spike"
  '';

  meta = {
    description = "THROWAWAY SPIKE: one Zone state (GAMES, Ready, Modern) drawn natively with Slint";
    mainProgram = "zone-native-spike";
    platforms = lib.platforms.linux;
  };
}

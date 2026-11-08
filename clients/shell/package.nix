# The native Korri shell, `korri-shell`, for the workstation or a device.
#
#   nix build .#korri-shell
#   nix build --impure --expr '(import <nixpkgs> { system = "aarch64-linux"; }).callPackage ./clients/shell/package.nix {}'
#
# Software renderer, winit on Wayland and X11. The tests run with the fixture
# korrid (feature `fixture`), which is never part of the installed output.
{ pkgs }:
let
  lib = pkgs.lib;
  # winit, xkbcommon and fontique open these with dlopen at run time; none is
  # linked. They go on the binary's RUNPATH so the store path runs as it is.
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
  pname = "korri-shell";
  version = "0.0.0";

  # The repository root: the treaty crate (contracts/surface) is a path
  # dependency. The pico8 palette test reads Pico's own palette, in
  # packages/pico.
  src = lib.fileset.toSource {
    root = ../..;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
      ./tests
      ./examples
      ./fixtures
      ./packages
      ./vendor
      ./window-sdl
      ../../contracts/surface/Cargo.toml
      ../../packages/input-trace
      ../../contracts/surface/src
    ];
  };
  buildAndTestSubdir = "clients/shell";
  cargoRoot = "clients/shell";
  cargoLock.lockFile = ./Cargo.lock;

  # The shipped binary is the `dist` profile (Cargo.toml); the tests run in release, as
  # `nix run .#shell-check` does.
  buildType = "dist";
  checkType = "release";
  checkFeatures = [ "fixture" ];
  # Some tests draw text that no package font covers: they need fontconfig (opened with
  # dlopen, as at run time) and one system font, which the build sandbox lacks.
  preCheck = ''
    export LD_LIBRARY_PATH="${lib.makeLibraryPath runtimeLibraries}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    export FONTCONFIG_FILE=${pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts ]; }}
  '';
  # The credential test needs a filesystem that keeps POSIX modes; the Nix
  # sandbox does. Every test is display-free.
  nativeBuildInputs = [ pkgs.patchelf ];

  postFixup = ''
    patchelf --add-rpath "${lib.makeLibraryPath runtimeLibraries}" "$out/bin/korri-shell"
  '';

  meta = {
    description = "Native Korri shell: surface packages (.slint views, QuickJS logic) over korrid";
    mainProgram = "korri-shell";
    platforms = lib.platforms.linux;
  };
}

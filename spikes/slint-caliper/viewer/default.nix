# THROWAWAY SPIKE: the Slint parts viewer (README.md here).
#
#   nix build .#slint-caliper-viewer
#   nix run .#slint-caliper-viewer -- [--packages DIR] [--devices ID,ID] [--part SUFFIX]
#
# Built like clients/shell/package.nix: the shell's vendored Slint 1.18.1, the
# software renderer, winit on Wayland and X11, the run-time libraries on RUNPATH.
{ pkgs }:
let
  lib = pkgs.lib;
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
  src = lib.fileset.toSource {
      root = ../../..;
      fileset = lib.fileset.unions [
        ./Cargo.toml
        ./Cargo.lock
        ./src
        ./ui
        ../../../clients/shell/Cargo.toml
        ../../../clients/shell/src
        ../../../clients/shell/examples
        ../../../clients/shell/tests
        ../../../clients/shell/fixtures
        ../../../clients/shell/packages
        ../../../clients/shell/vendor
        ../../../clients/shell/window-sdl
        ../../../contracts/surface/Cargo.toml
        ../../../contracts/surface/src
      ];
    };
  package = pkgs.rustPlatform.buildRustPackage {
    pname = "slint-caliper-viewer";
    version = "0.0.0";
    inherit src;
    buildAndTestSubdir = "spikes/slint-caliper/viewer";
    cargoRoot = "spikes/slint-caliper/viewer";
    cargoLock.lockFile = ./Cargo.lock;
    # A copy of the packages, for a machine with no checkout: used only when neither
    # --packages, KORRI_PACKAGES nor a clients/shell/packages above the working
    # directory names one. Reload on save needs the checkout's own folder.
    KORRI_VIEWER_FALLBACK_PACKAGES = "${placeholder "out"}/share/korri-packages";
    postInstall = ''
      mkdir -p $out/share
      cp -r ${src}/clients/shell/packages $out/share/korri-packages
    '';
    nativeBuildInputs = [ pkgs.patchelf ];
    postFixup = ''
      patchelf --add-rpath "${lib.makeLibraryPath runtimeLibraries}" "$out/bin/slint-caliper-viewer"
    '';
    meta = {
      description = "THROWAWAY SPIKE: Caliper's parts browser at true size for Korri's Slint packages";
      mainProgram = "slint-caliper-viewer";
      platforms = lib.platforms.linux;
    };
  };
in
{
  inherit package;
  apps.slint-caliper-viewer = {
    type = "app";
    program = "${package}/bin/slint-caliper-viewer";
    meta.description = "THROWAWAY SPIKE: Korri's Slint parts at true device size, reload on save";
  };
}

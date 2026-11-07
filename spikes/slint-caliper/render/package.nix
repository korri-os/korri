# Throwaway, workstation-only renderer. The output is portable through nix copy.
{ pkgs }:
let
  lib = pkgs.lib;
  fonts = pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts ]; };
in
pkgs.rustPlatform.buildRustPackage {
  pname = "slint-caliper-render";
  version = "0.0.0";
  src = lib.fileset.toSource {
    root = ../../..;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
      ./tests
      ./examples
      ../../../clients/shell/Cargo.toml
      ../../../clients/shell/src
      ../../../clients/shell/examples
      ../../../clients/shell/fixtures
      ../../../clients/shell/vendor
      ../../../clients/shell/window-sdl
      ../../../contracts/surface/Cargo.toml
      ../../../contracts/surface/src
    ];
  };
  cargoRoot = "spikes/slint-caliper/render";
  buildAndTestSubdir = "spikes/slint-caliper/render";
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.makeWrapper ];
  nativeCheckInputs = [ pkgs.python3 ];
  preCheck = ''
    export FONTCONFIG_FILE=${fonts}
    export LD_LIBRARY_PATH=${lib.makeLibraryPath [ pkgs.fontconfig.lib ]}
  '';
  postFixup = ''
    wrapProgram "$out/bin/slint-caliper-render" \
      --set FONTCONFIG_FILE ${fonts} \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath [ pkgs.fontconfig.lib ]}
  '';
  meta = {
    description = "Throwaway Caliper-style native Slint agent renderer";
    mainProgram = "slint-caliper-render";
    platforms = [ "x86_64-linux" ];
  };
}

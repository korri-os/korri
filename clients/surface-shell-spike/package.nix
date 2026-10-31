# THROWAWAY SPIKE. Built on workstation / fuji / yari, never on a device.
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
in
pkgs.rustPlatform.buildRustPackage {
  pname = "surface-shell-spike";
  version = "0.0.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
      (lib.fileset.fileFilter (file: file.hasExt "rs") ./verify)
      ./fixtures/model.json
      ./surfaces/_test
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.patchelf ];
  # winit / fontique open these libraries at runtime.
  postFixup = ''
    for executable in "$out"/bin/*; do
      patchelf --add-rpath "${lib.makeLibraryPath runtimeLibraries}" "$executable"
    done
  '';
  meta = {
    description = "THROWAWAY SPIKE: runtime Slint + QuickJS surface shell";
    mainProgram = "surface-shell-spike";
    platforms = lib.platforms.linux;
  };
}

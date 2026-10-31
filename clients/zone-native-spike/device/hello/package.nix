{ pkgs }:
pkgs.rustPlatform.buildRustPackage {
  pname = "zone-spike-hello";
  version = "0.0.0";
  src = pkgs.lib.cleanSource ./.;
  cargoLock.lockFile = ./Cargo.lock;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [
    pkgs.libxkbcommon
    pkgs.wayland
  ];
  # dlopen dependencies must be in the closure so bundle.sh can discover them.
  postFixup = ''
    patchelf --add-rpath ${
      pkgs.lib.makeLibraryPath [
        pkgs.wayland
        pkgs.libxkbcommon
        pkgs.fontconfig
      ]
    } "$out/bin/zone-spike-hello"
  '';
}

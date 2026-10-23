{ pkgs, wpewebkit }:
pkgs.stdenv.mkDerivation {
  pname = "korri-portal-wpe";
  version = "0.1.0";
  src = ./src;
  strictDeps = true;
  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [
    wpewebkit
    pkgs.glib
    pkgs.libsoup_3
    pkgs.libwpe
    pkgs.libxkbcommon
  ];
  buildPhase = ''
    runHook preBuild
    $CC -std=c11 -Wall -Wextra -Werror -O2 -D_FORTIFY_SOURCE=3 \
      $(pkg-config --cflags wpe-webkit-2.0 wpe-platform-2.0) \
      main.c -o korri-portal-wpe \
      $(pkg-config --libs wpe-webkit-2.0 wpe-platform-2.0)
    runHook postBuild
  '';
  installPhase = ''
    install -Dm755 korri-portal-wpe $out/bin/korri-portal-wpe
  '';
  meta = {
    description = "Private WPE startup for the Korri RPC bridge";
    mainProgram = "korri-portal-wpe";
    platforms = pkgs.lib.platforms.linux;
  };
}

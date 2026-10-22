{ stdenv }:
stdenv.mkDerivation {
  pname = "h700-venc-controls-probe";
  version = "1";
  src = ./controls-probe.c;
  dontUnpack = true;
  buildPhase = ''
    $CC -std=c11 -O2 -Wall -Wextra -Werror "$src" -o controls-probe
  '';
  installPhase = ''
    install -Dm755 controls-probe $out/bin/controls-probe
  '';
}

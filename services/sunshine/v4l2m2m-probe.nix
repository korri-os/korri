{ pkgs, ffmpeg }:
pkgs.stdenv.mkDerivation {
  pname = "sunshine-v4l2m2m-probe";
  version = "1";
  src = ./test-v4l2m2m-encode.c;
  dontUnpack = true;
  buildPhase = ''
    $CC -std=c11 -O2 -Wall -Wextra -Werror -I${ffmpeg}/include "$src" \
      ${ffmpeg}/lib/libavcodec.a ${ffmpeg}/lib/libavutil.a -lm -lpthread \
      -o sunshine-v4l2m2m-probe
  '';
  installPhase = ''
    install -Dm755 sunshine-v4l2m2m-probe "$out/bin/sunshine-v4l2m2m-probe"
  '';
  meta.mainProgram = "sunshine-v4l2m2m-probe";
}

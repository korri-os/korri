# Exercise the actual functions from the product kernel's patched source.
# No tablet or ADSP is needed; this checks PCM ring reuse, not hardware timing.
{ pkgs, source }:
pkgs.runCommand "tabs6-q6asm-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.stdenv.cc
    ];
  }
  ''
    python3 ${./q6asm-reprepare.test.py} ${source}
    touch "$out"
  ''

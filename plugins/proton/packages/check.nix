# Focused source checks. Invoke with build-host pkgs; tests unpack a small
# synthetic release, never the internet, and no device.
{ pkgs }:
pkgs.runCommand "cachyos-unpack-check"
  {
    nativeBuildInputs = with pkgs; [
      bash
      coreutils
      gnugrep
      gnutar
      python3
      shellcheck
      xz
    ];
  }
  ''
    cp -r ${../cachyos} cachyos
    chmod -R u+w cachyos
    (cd cachyos && python3 -m unittest -v test_unpack)
    bash -n cachyos/unpack.sh
    shellcheck -s bash cachyos/unpack.sh
    touch "$out"
  ''

# Focused source checks. Invoke with build-host pkgs; tests use a local
# fixture server, never the internet, and no device.
{ pkgs }:
pkgs.runCommand "cachyos-prepare-check"
  {
    nativeBuildInputs = with pkgs; [
      bash
      coreutils
      curl
      gnugrep
      gnutar
      python3
      shellcheck
      util-linux
      xz
    ];
  }
  ''
    cp -r ${../cachyos} cachyos
    chmod -R u+w cachyos
    (cd cachyos && python3 -m unittest -v test_prepare)
    bash -n cachyos/prepare.sh
    shellcheck -s bash cachyos/prepare.sh
    touch "$out"
  ''

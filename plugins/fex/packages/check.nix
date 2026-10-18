# Focused source checks. Invoke with build-host pkgs; tests use a local
# fixture server, never the internet, and no device.
{ pkgs }:
pkgs.runCommand "fex-prepare-check"
  {
    nativeBuildInputs = with pkgs; [
      bash
      coreutils
      curl
      gnutar
      python3
      shellcheck
      squashfsTools
      util-linux
      zstd
    ];
  }
  ''
    cp -r ${../rootfs} rootfs
    chmod -R u+w rootfs
    (cd rootfs && python3 -m unittest -v test_prepare)
    bash -n rootfs/prepare.sh
    shellcheck -s bash rootfs/prepare.sh
    touch "$out"
  ''

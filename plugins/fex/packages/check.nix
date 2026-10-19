# Focused source checks. Invoke with build-host pkgs; tests unpack small
# synthetic archives, never the internet, and no device.
{ pkgs }:
pkgs.runCommand "fex-unpack-check"
  {
    nativeBuildInputs = with pkgs; [
      bash
      coreutils
      gnutar
      python3
      shellcheck
      squashfsTools
      zstd
    ];
  }
  ''
    cp -r ${../rootfs} rootfs
    chmod -R u+w rootfs
    (cd rootfs && python3 -m unittest -v test_unpack)
    bash -n rootfs/unpack.sh
    shellcheck -s bash rootfs/unpack.sh
    touch "$out"
  ''

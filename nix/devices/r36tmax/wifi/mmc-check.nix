{ pkgs, kernel ? pkgs.callPackage ../dts/kernel-trimmed.nix { } }:
pkgs.runCommand "r36tmax-mmc-parser-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.stdenv.cc
      pkgs.patch
      pkgs.xz
    ];
  }
  ''
    mkdir linux
    tar -xJf ${kernel.src} -C linux --strip-components=1 \
      linux-${kernel.version}/drivers/mmc/core/host.c \
      linux-${kernel.version}/drivers/mmc/core/sdio.c \
      linux-${kernel.version}/drivers/mmc/core/sdio_cis.c \
      linux-${kernel.version}/drivers/mmc/host/dw_mmc.c \
      linux-${kernel.version}/include/linux/mmc/host.h
    patch --batch --fuzz=0 -d linux -p1 < ${./mmc-support.patch}
    python3 ${./check-cis.py} linux
    touch "$out"
  ''

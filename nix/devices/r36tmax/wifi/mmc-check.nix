{ pkgs }:
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
    tar -xJf ${pkgs.linux_6_12.src} -C linux --strip-components=1 \
      linux-${pkgs.linux_6_12.version}/drivers/mmc/core/host.c \
      linux-${pkgs.linux_6_12.version}/drivers/mmc/core/sdio.c \
      linux-${pkgs.linux_6_12.version}/drivers/mmc/core/sdio_cis.c \
      linux-${pkgs.linux_6_12.version}/drivers/mmc/host/dw_mmc.c \
      linux-${pkgs.linux_6_12.version}/include/linux/mmc/host.h
    patch --batch --fuzz=0 -d linux -p1 < ${./mmc-support.patch}
    python3 ${./check-cis.py} linux
    touch "$out"
  ''

# Compile only the real board DTB with host tools; never build a kernel.
{ pkgs, kernel ? pkgs.callPackage ./kernel-trimmed.nix { } }:
pkgs.runCommand "r36tmax-game-buttons-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.stdenv.cc
      pkgs.dtc
      pkgs.xz
    ];
  }
  ''
    mkdir linux
    tar -xJf ${kernel.src} -C linux --strip-components=1 \
      linux-${kernel.version}/arch/arm64/boot/dts/rockchip \
      linux-${kernel.version}/include/dt-bindings \
      linux-${kernel.version}/include/uapi/linux/input-event-codes.h
    cpp -nostdinc -undef -D__DTS__ -x assembler-with-cpp \
      -I linux/include -I linux/arch/arm64/boot/dts/rockchip \
      ${./rk3326-aislpc-r36t-max.dts} > board.dts
    mkdir -p "$out/rockchip"
    dtb="$out/rockchip/rk3326-aislpc-r36t-max.dtb"
    dtc -I dts -O dtb -o "$dtb" board.dts
    python3 ${./test-game-buttons.py} "$dtb" ${./check-game-buttons.py}
    python3 ${./test-speaker.py} "$dtb" ${./check-speaker.py}
    python3 ${../wifi/check-dtb.py} "$dtb"
  ''

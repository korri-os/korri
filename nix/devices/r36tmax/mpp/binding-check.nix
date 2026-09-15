# Compile the actual board plus the disabled offline candidate on the host.
# Do not add this overlay to any NixOS image or load it into a running kernel.
{ pkgs }:
pkgs.runCommand "r36tmax-mpp-binding-check"
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
    tar -xJf ${pkgs.linux_6_12.src} -C linux --strip-components=1 \
      linux-${pkgs.linux_6_12.version}/arch/arm64/boot/dts/rockchip \
      linux-${pkgs.linux_6_12.version}/include/dt-bindings \
      linux-${pkgs.linux_6_12.version}/include/uapi/linux/input-event-codes.h
    cpp -nostdinc -undef -D__DTS__ -x assembler-with-cpp \
      -I linux/include -I linux/arch/arm64/boot/dts/rockchip \
      ${../dts/rk3326-aislpc-r36t-max.dts} > board.dts
    dtc -@ -I dts -O dtb -o board.dtb board.dts
    cpp -nostdinc -undef -D__DTS__ -x assembler-with-cpp \
      -I linux/include ${./rk-mpp-service-overlay.dts} > overlay.dts
    dtc -@ -I dts -O dtb -o overlay.dtbo overlay.dts
    fdtoverlay -i board.dtb -o candidate.dtb overlay.dtbo
    python3 ${../dts/test-mpp.py} candidate.dtb ${../dts/check-mpp.py}
    python3 ${../dts/test-game-buttons.py} candidate.dtb ${../dts/check-game-buttons.py}
    python3 ${../dts/test-speaker.py} candidate.dtb ${../dts/check-speaker.py}
    python3 ${../wifi/check-dtb.py} candidate.dtb
    mkdir -p "$out"
    cp board.dtb "$out/baseline.dtb"
    cp candidate.dtb "$out/disabled-candidate.dtb"
  ''

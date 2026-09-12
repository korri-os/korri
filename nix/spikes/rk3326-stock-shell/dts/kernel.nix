# A mainline kernel carrying the R36T Max board device tree.
#
# Stock mainline on purpose. The panel needs an ST7703 variant with this
# board's init sequence, but adding it here would mean a kernel rebuild per
# iteration and would confuse two questions into one: whether the board boots
# at all, and whether the panel lights. The RG353M already shows the answer —
# `nix/devices/rg353m/st7703-panel-module.nix` builds that single driver out
# of tree — so the panel comes later and separately.
#
# Expect no display from this kernel. A serial console on UART5 and a device
# that reaches userspace is the whole goal.
{
  lib,
  linuxPackages,
}:

let
  dtbName = "rk3326-aislpc-r36t-max";
in
linuxPackages.kernel.overrideAttrs (old: {
  postPatch = (old.postPatch or "") + ''
    cp ${./rk3326-aislpc-r36t-max.dts} arch/arm64/boot/dts/rockchip/${dtbName}.dts
    echo 'dtb-$(CONFIG_ARCH_ROCKCHIP) += ${dtbName}.dtb' \
      >> arch/arm64/boot/dts/rockchip/Makefile
  '';

  passthru = (old.passthru or { }) // { inherit dtbName; };
})

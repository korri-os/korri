# Check the generated Kconfig, not merely the requested patch attributes.
{ pkgs, configuration }:
let
  cfg = configuration.config;
in
assert cfg.hardware.bluetooth.enable;
assert builtins.elem "bluetooth.target" cfg.systemd.services.bluetooth.wantedBy;
assert cfg.hardware.deviceTree.name == "rockchip/rk3566-anbernic-rg353p.dtb";
pkgs.runCommand "rg353m-bluetooth-check" { } ''
  for setting in \
    CONFIG_BT_HCIUART=m \
    CONFIG_BT_RTL=m \
    CONFIG_SERIAL_DEV_BUS=y \
    CONFIG_BT_HCIUART_SERDEV=y \
    CONFIG_BT_HCIUART_3WIRE=y \
    CONFIG_BT_HCIUART_RTL=y
  do
    grep -Fx "$setting" ${cfg.boot.kernelPackages.kernel.configfile}
  done
  touch "$out"
''

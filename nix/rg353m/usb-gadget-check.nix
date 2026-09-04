{
  configuration,
  lib,
  pkgs,
  runCommand,
}:

let
  devicePackage = configuration.pkgs.callPackage ./usb-gadget-package.nix { };
  testPackage = pkgs.callPackage ./usb-gadget-package.nix {
    configfsRoot = "$TMPDIR/configfs";
    udcRoot = "$TMPDIR/udc";
  };
  service = configuration.config.systemd.services.usb-gadget;
in
assert service.serviceConfig.ExecStart == lib.getExe devicePackage;
assert lib.hasInfix ''
  if ! echo "" > /sys/kernel/config/usb_gadget/rg353m/UDC; then
'' service.preStop;
runCommand "rg353m-usb-gadget-check"
  {
    nativeBuildInputs = [ testPackage ];
  }
  ''
    set -euo pipefail

    mkdir -p "$TMPDIR/configfs" "$TMPDIR/udc/fcc00000.usb"

    rg353m-usb-gadget-configure
    gadget="$TMPDIR/configfs/rg353m"
    test "$(cat "$gadget/UDC")" = fcc00000.usb
    test "$(cat "$gadget/functions/ncm.usb0/host_addr")" = 02:52:47:35:33:01
    test "$(cat "$gadget/functions/ncm.usb0/dev_addr")" = 02:52:47:35:33:02
    test -L "$gadget/configs/c.1/ncm.usb0"
    test -L "$gadget/configs/c.1/acm.usb0"

    : > "$gadget/UDC"
    rg353m-usb-gadget-configure
    test "$(cat "$gadget/UDC")" = fcc00000.usb
    test "$(cat "$gadget/functions/ncm.usb0/host_addr")" = 02:52:47:35:33:01
    test "$(cat "$gadget/functions/ncm.usb0/dev_addr")" = 02:52:47:35:33:02

    : > "$gadget/UDC"
    printf '%s\n' 02:00:00:00:00:01 > "$gadget/functions/ncm.usb0/host_addr"
    if rg353m-usb-gadget-configure > mismatch.stdout 2> mismatch.stderr; then
      echo "mismatched existing NCM address unexpectedly succeeded" >&2
      exit 1
    fi
    grep -F 'expected 02:52:47:35:33:01' mismatch.stderr
    test ! -s "$gadget/UDC"

    touch "$out"
  ''

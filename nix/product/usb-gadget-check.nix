# Shell control-flow checks for the product USB gadget program, with ordinary
# directories. These cannot prove configfs semantics, function registration,
# UDC binding or USB traffic; usb-gadget-vm-test.nix covers real configfs.
# Moved from the RP Mini V2 module check, plus the named-controller case the
# RG353M needs (two controllers probe there).
{ pkgs }:
let
  # One UDC wait second keeps the failure cases short.
  testPackage = pkgs.callPackage ./usb-gadget-package.nix {
    configfsRoot = "$TMPDIR/configfs";
    udcRoot = "$TMPDIR/udc";
    udcWaitSeconds = 1;
  };
in
pkgs.runCommand "korri-product-usb-gadget-check" { nativeBuildInputs = [ testPackage ]; } ''
  set -euo pipefail
  export KORRI_USB_GADGET_NAME=device
  export KORRI_USB_GADGET_PRODUCT="Device NixOS"
  export KORRI_USB_GADGET_HOST_MAC=02:52:50:4D:32:01
  export KORRI_USB_GADGET_DEVICE_MAC=02:52:50:4d:32:02
  export KORRI_USB_GADGET_UDC=
  gadget_dir="$TMPDIR/configfs/device"
  mkdir -p "$gadget_dir"
  : > "$gadget_dir/UDC" # configfs supplies this attribute; plain dirs do not.

  # Missing class directory: wait, fail, and leave the gadget unbound.
  if korri-usb-gadget-configure > none.stdout 2> none.stderr; then
    echo "gadget bound without a USB device controller" >&2
    exit 1
  fi
  grep -F 'no USB device controller appeared' none.stderr
  test ! -s "$gadget_dir/UDC"
  # Existing but empty class directory follows the same timeout path.
  mkdir "$TMPDIR/udc"
  if korri-usb-gadget-configure > empty.stdout 2> empty.stderr; then
    echo "gadget bound with an empty controller class" >&2
    exit 1
  fi
  grep -F 'no USB device controller appeared' empty.stderr

  # One controller binds. Upper-case configuration is stored in lower case.
  mkdir "$TMPDIR/udc/a600000.usb"
  korri-usb-gadget-configure
  test "$(cat "$gadget_dir/UDC")" = a600000.usb
  test "$(cat "$gadget_dir/functions/ncm.usb0/host_addr")" = 02:52:50:4d:32:01
  test "$(cat "$gadget_dir/functions/ncm.usb0/dev_addr")" = 02:52:50:4d:32:02
  test "$(cat "$gadget_dir/strings/0x409/serialnumber")" = device-nixos
  test "$(cat "$gadget_dir/strings/0x409/product")" = "Device NixOS"
  test "$gadget_dir/configs/c.1/ncm.usb0" -ef "$gadget_dir/functions/ncm.usb0"
  test "$gadget_dir/configs/c.1/acm.usb0" -ef "$gadget_dir/functions/acm.usb0"
  # A second invocation must not nest links inside either function.
  korri-usb-gadget-configure
  test ! -e "$gadget_dir/functions/ncm.usb0/ncm.usb0"
  test ! -e "$gadget_dir/functions/acm.usb0/acm.usb0"
  # A restart after preStop's unbind binds the same controller again.
  : > "$gadget_dir/UDC"
  korri-usb-gadget-configure
  test "$(cat "$gadget_dir/UDC")" = a600000.usb

  # A second controller is ambiguous without a named one: refuse, name both.
  : > "$gadget_dir/UDC"
  mkdir "$TMPDIR/udc/a800000.usb"
  if korri-usb-gadget-configure > two.stdout 2> two.stderr; then
    echo "gadget guessed between two USB device controllers" >&2
    exit 1
  fi
  grep -F 'found: a600000.usb a800000.usb' two.stderr
  test ! -s "$gadget_dir/UDC"

  # A named controller binds even when another one is present.
  KORRI_USB_GADGET_UDC=a800000.usb korri-usb-gadget-configure
  test "$(cat "$gadget_dir/UDC")" = a800000.usb
  # A named controller that never appears fails and names it.
  : > "$gadget_dir/UDC"
  if KORRI_USB_GADGET_UDC=fcc00000.usb korri-usb-gadget-configure > named.stdout 2> named.stderr; then
    echo "gadget bound without its named controller" >&2
    exit 1
  fi
  grep -F 'USB device controller fcc00000.usb did not appear' named.stderr
  test ! -s "$gadget_dir/UDC"
  rmdir "$TMPDIR/udc/a800000.usb"

  # A changed existing NCM address is refused before the bind.
  printf '%s\n' 02:00:00:00:00:01 > "$gadget_dir/functions/ncm.usb0/host_addr"
  if korri-usb-gadget-configure > mismatch.stdout 2> mismatch.stderr; then
    echo "mismatched existing NCM address unexpectedly succeeded" >&2
    exit 1
  fi
  grep -F 'expected 02:52:50:4d:32:01' mismatch.stderr
  test ! -s "$gadget_dir/UDC"
  touch "$out"
''

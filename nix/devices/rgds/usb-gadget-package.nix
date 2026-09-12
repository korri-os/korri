# Compose the RG DS USB gadget through configfs: one cable carries an NCM
# network link and the ACM console. The RG353M has the same shape with its own
# identifiers; the two devices keep separate gadget names and addresses so both
# can be attached to one workstation at once.
{
  pkgs,
  configfsRoot ? "/sys/kernel/config/usb_gadget",
  udcRoot ? "/sys/class/udc",
  hostMac ? "02:52:47:52:47:01",
  deviceMac ? "02:52:47:52:47:02",
}:

pkgs.writeShellApplication {
  name = "rgds-usb-gadget-configure";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.findutils
  ];
  text = ''
    set -euo pipefail

    readonly gadget_root="${configfsRoot}"
    readonly udc_root="${udcRoot}"
    readonly gadget="$gadget_root/rgds"
    readonly expected_host_mac=${hostMac}
    readonly expected_device_mac=${deviceMac}

    verify_value() {
      local path="$1"
      local expected="$2"
      local actual
      actual="$(cat "$path")"
      if [ "$actual" != "$expected" ]; then
        printf '%s is %s, expected %s\n' "$path" "$actual" "$expected" >&2
        exit 1
      fi
    }

    mkdir -p "$gadget"
    cd "$gadget"
    echo 0x1d6b > idVendor
    echo 0x0104 > idProduct
    echo 0x0100 > bcdDevice
    echo 0x0200 > bcdUSB
    mkdir -p strings/0x409
    echo "rgds-nixos" > strings/0x409/serialnumber
    echo "Korri" > strings/0x409/manufacturer
    echo "RG DS NixOS" > strings/0x409/product
    mkdir -p configs/c.1/strings/0x409
    echo "NCM + ACM" > configs/c.1/strings/0x409/configuration
    echo 250 > configs/c.1/MaxPower
    if [ ! -d functions/ncm.usb0 ]; then
      mkdir -p functions/ncm.usb0
      echo "$expected_host_mac" > functions/ncm.usb0/host_addr
      echo "$expected_device_mac" > functions/ncm.usb0/dev_addr
    else
      verify_value functions/ncm.usb0/host_addr "$expected_host_mac"
      verify_value functions/ncm.usb0/dev_addr "$expected_device_mac"
    fi
    mkdir -p functions/acm.usb0
    ln -sf functions/ncm.usb0 configs/c.1/
    ln -sf functions/acm.usb0 configs/c.1/
    udc="$(find "$udc_root" -mindepth 1 -maxdepth 1 -printf '%f\n' | head -n 1)"
    test -n "$udc"
    echo "$udc" > UDC
  '';
}

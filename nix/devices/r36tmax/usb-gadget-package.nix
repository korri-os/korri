# Compose the R36T Max USB gadget through configfs: one cable carries an NCM
# network link and the ACM console. The RG353M has the same shape with its own
# identifiers; the two devices keep separate gadget names and addresses so both
# can be attached to one workstation at once.
{
  pkgs,
  configfsRoot ? "/sys/kernel/config/usb_gadget",
  udcRoot ? "/sys/class/udc",
  udcWaitSeconds ? 60,
  hostMac ? "02:52:33:36:54:01",
  deviceMac ? "02:52:33:36:54:02",
}:

pkgs.writeShellApplication {
  name = "r36tmax-usb-gadget-configure";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.findutils
  ];
  text = ''
    set -euo pipefail

    readonly gadget_root="${configfsRoot}"
    readonly udc_root="${udcRoot}"
    readonly gadget="$gadget_root/r36tmax"
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
    echo "r36tmax-nixos" > strings/0x409/serialnumber
    echo "Korri" > strings/0x409/manufacturer
    echo "R36T Max NixOS" > strings/0x409/product
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
    # The controller can register after this service starts: the first boot of
    # the composed gadget on the RG DS found /sys/class/udc still empty one
    # second in. Wait for it, and say so on failure. A bare test exits silently,
    # which took the USB console down with nothing in the journal to explain it.
    udc=""
    for _ in $(seq 1 ${toString udcWaitSeconds}); do
      udc="$(find "$udc_root" -mindepth 1 -maxdepth 1 -printf '%f\n' | head -n 1)"
      if [ -n "$udc" ]; then
        break
      fi
      sleep 1
    done
    if [ -z "$udc" ]; then
      printf 'no USB device controller appeared in %s after %s seconds\n' \
        "$udc_root" ${toString udcWaitSeconds} >&2
      exit 1
    fi
    echo "$udc" > UDC
  '';
}

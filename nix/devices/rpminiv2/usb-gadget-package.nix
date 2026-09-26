# Compose the RP Mini V2 product USB gadget through configfs: one cable carries
# an NCM network link and the ACM root console. The shape and identifiers follow
# the RG353M, RG DS and R36T Max gadgets. This device keeps its own gadget name
# and addresses so all of them can be attached to one workstation at once.
{
  pkgs,
  configfsRoot ? "/sys/kernel/config/usb_gadget",
  udcRoot ? "/sys/class/udc",
  udcWaitSeconds ? 60,
  # "RPM2" in ASCII after the shared 02 prefix. The kernel reports these
  # addresses in lower case, so the comparison below needs lower case too.
  hostMac ? "02:52:50:4d:32:01",
  deviceMac ? "02:52:50:4d:32:02",
}:

pkgs.writeShellApplication {
  name = "rpminiv2-usb-gadget-configure";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.findutils
  ];
  text = ''
    set -euo pipefail

    readonly gadget_root="${configfsRoot}"
    readonly udc_root="${udcRoot}"
    readonly gadget="$gadget_root/rpminiv2"
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
    echo "rpminiv2-nixos" > strings/0x409/serialnumber
    echo "Korri" > strings/0x409/manufacturer
    echo "RP Mini V2 NixOS" > strings/0x409/product
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
    # The device tree enables one controller, usb@a600000 (dwc3, dr_mode otg
    # with a role switch), so exactly one UDC is expected. It can register after
    # this service starts, as it did on the RG DS. Wait for it. Refuse to guess
    # if a second controller appears, and report every failure in the journal:
    # this gadget carries the only USB root console on the product image.
    udcs=""
    for _ in $(seq 1 ${toString udcWaitSeconds}); do
      udcs="$(find "$udc_root" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort)"
      if [ -n "$udcs" ]; then
        break
      fi
      sleep 1
    done
    if [ -z "$udcs" ]; then
      printf 'no USB device controller appeared in %s after %s seconds\n' \
        "$udc_root" ${toString udcWaitSeconds} >&2
      exit 1
    fi
    if [ "$(printf '%s\n' "$udcs" | wc -l)" -ne 1 ]; then
      printf 'expected one USB device controller in %s, found: %s\n' \
        "$udc_root" "$(printf '%s' "$udcs" | tr '\n' ' ')" >&2
      exit 1
    fi
    printf 'binding the RP Mini V2 USB gadget to %s\n' "$udcs"
    echo "$udcs" > UDC
  '';
}

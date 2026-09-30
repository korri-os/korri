# Compose the product USB gadget through configfs: one cable carries an NCM
# network link and the ACM root console. Extracted from the RP Mini V2 script,
# the strictest of the four device copies (RG353M, RG DS, R36T Max, Mini V2).
#
# The program is the same on every device, so the product check can compare it
# exactly. Device identity arrives as unit environment:
#   KORRI_USB_GADGET_NAME     configfs gadget and serial-number stem
#   KORRI_USB_GADGET_PRODUCT  USB product string
#   KORRI_USB_GADGET_HOST_MAC / KORRI_USB_GADGET_DEVICE_MAC  NCM addresses
#   KORRI_USB_GADGET_UDC      optional: the one controller to bind; empty means
#                             exactly one controller must appear
{
  pkgs,
  configfsRoot ? "/sys/kernel/config/usb_gadget",
  udcRoot ? "/sys/class/udc",
  udcWaitSeconds ? 60,
}:

pkgs.writeShellApplication {
  name = "korri-usb-gadget-configure";
  runtimeInputs = [
    pkgs.coreutils
    pkgs.findutils
    pkgs.gnugrep
  ];
  text = ''
    set -euo pipefail

    readonly name="''${KORRI_USB_GADGET_NAME:?}"
    readonly product="''${KORRI_USB_GADGET_PRODUCT:?}"
    # The kernel reports these addresses in lower case, so compare lower case.
    expected_host_mac="$(printf '%s' "''${KORRI_USB_GADGET_HOST_MAC:?}" | tr 'A-F' 'a-f')"
    expected_device_mac="$(printf '%s' "''${KORRI_USB_GADGET_DEVICE_MAC:?}" | tr 'A-F' 'a-f')"
    readonly expected_host_mac expected_device_mac
    readonly wanted_udc="''${KORRI_USB_GADGET_UDC:-}"
    readonly gadget_root="${configfsRoot}"
    readonly udc_root="${udcRoot}"
    readonly gadget="$gadget_root/$name"

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

    link_function() {
      local function="$1"
      local link="configs/c.1/$function"
      if [ -L "$link" ]; then
        # Configfs generates its own relative link text. Compare the targets,
        # not that text. Relinking an existing function can fail even with
        # ln -f: configfs permits each function only once per configuration.
        if [ ! "$link" -ef "functions/$function" ]; then
          printf 'unexpected function link: %s\n' "$link" >&2
          exit 1
        fi
      else
        # Configfs resolves relative targets from cwd at creation time,
        # unlike a normal filesystem. An absolute target works in both.
        ln -s "$gadget/functions/$function" "$link"
      fi
    }

    mkdir -p "$gadget"
    cd "$gadget"
    echo 0x1d6b > idVendor
    echo 0x0104 > idProduct
    echo 0x0100 > bcdDevice
    echo 0x0200 > bcdUSB
    mkdir -p strings/0x409
    echo "$name-nixos" > strings/0x409/serialnumber
    echo "Korri" > strings/0x409/manufacturer
    echo "$product" > strings/0x409/product
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
    link_function ncm.usb0
    link_function acm.usb0

    # The controller can register after this service starts, as it did on the
    # RG DS. Wait for it. Refuse to guess between controllers, and report every
    # failure in the journal: this gadget carries the only USB root console.
    udcs=""
    for _ in $(seq 1 ${toString udcWaitSeconds}); do
      # The class directory itself may not exist before the driver loads.
      if [ -d "$udc_root" ]; then
        udcs="$(find "$udc_root" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort)"
      fi
      if [ -n "$wanted_udc" ]; then
        if printf '%s\n' "$udcs" | grep -qxF "$wanted_udc"; then
          udcs="$wanted_udc"
          break
        fi
      elif [ -n "$udcs" ]; then
        break
      fi
      sleep 1
    done
    if [ -n "$wanted_udc" ] && [ "$udcs" != "$wanted_udc" ]; then
      printf 'USB device controller %s did not appear in %s after %s seconds\n' \
        "$wanted_udc" "$udc_root" ${toString udcWaitSeconds} >&2
      exit 1
    fi
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
    # Writing even the same name to a bound configfs UDC returns EBUSY.
    # Preserve an already-correct binding without disconnecting the console.
    if [ -n "$(cat UDC)" ]; then
      verify_value UDC "$udcs"
    else
      printf 'binding the %s USB gadget to %s\n' "$name" "$udcs"
      echo "$udcs" > UDC
    fi
  '';
}

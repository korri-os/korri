G=/sys/kernel/config/usb_gadget/tabs6
modprobe libcomposite 2>/dev/null || true

# Put the port in device role explicitly, so the lifeline never depends on
# the dwc3 default (dr_mode is otg with a userspace role switch; host mode is
# a deliberate act, dwc3-allow-userspace-role-control.patch). In host role
# there is no UDC, so this comes before the UDC wait. The switch registers
# when dwc3 probes, which can be late.
for _ in $(seq 1 100); do
  for r in /sys/class/usb_role/*/role; do
    [ -e "$r" ] || continue
    if [ "$(cat "$r")" != device ]; then
      echo device > "$r" && echo "usb role: ${r%/role} -> device"
    else
      echo "usb role: ${r%/role} is device"
    fi
    break 2
  done
  sleep 0.2
done

if [ -e "$G/UDC" ] && [ -n "$(cat "$G/UDC")" ]; then
  echo "gadget already bound to $(cat "$G/UDC")"
  exit 0
fi

# The SM8150 dwc3 controller. It can appear late: dwc3 defers until the
# QMP combo PHY probes (USB_NETWORKING.md section 1).
UDC=""
for _ in $(seq 1 100); do
  for u in /sys/class/udc/*; do
    [ -e "$u" ] && UDC=${u##*/}
  done
  [ -n "$UDC" ] && break
  sleep 0.2
done
[ -n "$UDC" ] || { echo "no UDC appeared" >&2; exit 1; }

mkdir -p "$G"
cd "$G"
# Linux Foundation multifunction composite gadget IDs. Bump idProduct
# when the function set changes: Windows caches descriptors per VID:PID.
echo 0x1d6b > idVendor
echo 0x0104 > idProduct
echo 0x0100 > bcdDevice
echo 0x0200 > bcdUSB
# IAD, so Windows accepts the composite device.
echo 0xEF > bDeviceClass
echo 0x02 > bDeviceSubClass
echo 0x01 > bDeviceProtocol

mkdir -p strings/0x409
echo tabs6 > strings/0x409/serialnumber
echo Samsung > strings/0x409/manufacturer
echo "Galaxy Tab S6 NixOS" > strings/0x409/product

# Microsoft OS descriptors: Windows binds its in-box RNDIS driver.
echo 1 > os_desc/use
echo 0xcd > os_desc/b_vendor_code
echo MSFT100 > os_desc/qw_sign

mkdir -p functions/rndis.usb0
# Locally administered, fixed, so the host keeps one interface name.
echo 02:7a:b6:00:00:01 > functions/rndis.usb0/dev_addr
echo 02:7a:b6:00:00:02 > functions/rndis.usb0/host_addr
echo RNDIS > functions/rndis.usb0/os_desc/interface.rndis/compatible_id
echo 5162001 > functions/rndis.usb0/os_desc/interface.rndis/sub_compatible_id

mkdir -p functions/acm.GS0

mkdir -p configs/c.1/strings/0x409
echo "RNDIS + ACM" > configs/c.1/strings/0x409/configuration
echo 250 > configs/c.1/MaxPower
# The MS OS descriptors name the configuration: the link lives in os_desc.
# (`ln -s os_desc configs/c.1` is EINVAL in configfs.)
[ -e os_desc/c.1 ] || ln -s configs/c.1 os_desc
[ -e configs/c.1/rndis.usb0 ] || ln -s functions/rndis.usb0 configs/c.1/
[ -e configs/c.1/acm.GS0 ] || ln -s functions/acm.GS0 configs/c.1/

echo "$UDC" > UDC
echo "gadget bound to $UDC"

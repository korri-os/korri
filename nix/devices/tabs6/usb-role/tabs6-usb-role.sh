# tabs6-usb-role: turn the Tab S6 USB-C port between the gadget lifeline
# (device role) and a USB host that powers the port (a gamepad, keyboard, hub).
#
#   tabs6-usb-role [status]   the role, VBUS and attached USB devices
#   tabs6-usb-role host       power the port, then become a USB host
#   tabs6-usb-role device     become the gadget again, VBUS off, rebind the gadget
#   tabs6-usb-role boot-role  device role, before the gadget binds (boot)
#   tabs6-usb-role boot-vbus  VBUS off (boot)
#
# Ported from the reference port's tools/usb-role (docs/USB_HOST.md). The
# port is dual-role in the device tree (dr_mode = "otg", usb-role-switch) and
# dwc3 lets userspace drive the switch. Nothing on this board says what is
# plugged in: there is no Type-C port manager (the s2mm005 PD controller has
# no mainline driver) and no ID pin. A bus-powered device gets no power from
# the role switch alone: VBUS comes from the SM5705 charger's boost, which has
# no kernel driver, so it is set over I2C here. Register map from the vendor
# driver, cross-checked against the live part by the port (USB_HOST.md 2):
#
#   0x0c CNTL       bits 2:0   operation mode: 5 = CHG_ON, 7 = USB_OTG
#   0x14 CHGCNTL6   bits 3:2   OTG current limit: 2 = 900 mA
#   0x20 FLEDCNTL6  bits 3:0   boost output: 5 = 4500 mV, 0xb = 5100 mV
#
# VBUS is never sourced while anything else may be powering the port. The
# port refused only while the gauge said Charging; this refuses unless it says
# Discharging (a host can feed less than the tablet draws, and Full or Not
# charging still mean external power), unless the gauge is missing, and while
# a USB host has the gadget configured.
#
# The test (usb-role.test.py) runs this file against a temporary sysfs tree.
# Paths and timings therefore come from the environment:
#   TABS6_SYSFS                    sysfs root (/sys)
#   TABS6_USB_GADGET_UNIT          unit that binds the gadget; restarted after
#                                  the port returns to device role
#   TABS6_I2C_DEV                  the SM5705's I2C device node (/dev/i2c-1)
#   TABS6_USB_ROLE_WAIT_S          boot-*: how long to wait for the switch and
#                                  the I2C bus (20)
set -eu

SYSFS=${TABS6_SYSFS:-/sys}
GADGET_UNIT=${TABS6_USB_GADGET_UNIT:-}
I2C_DEV=${TABS6_I2C_DEV:-/dev/i2c-1}
WAIT_S=${TABS6_USB_ROLE_WAIT_S:-20}

CHG_BUS=1
CHG_ADDR=0x49
REG_CNTL=0x0c
REG_CHGCNTL6=0x14
REG_FLEDCNTL6=0x20

OP_MODE_CHG_ON=5
OP_MODE_USB_OTG=7
BST_OUT_5100MV=0x0b
BST_OUT_4500MV=0x05
OTG_CURRENT_900MA=2

FUELGAUGE=$SYSFS/class/power_supply/sm5705-fuelgauge

log() { echo "tabs6-usb-role: $*"; }
die() {
	echo "tabs6-usb-role: $*" >&2
	exit 1
}

rmw() { # register, mask, value
	local old new
	old=$(i2cget -y $CHG_BUS $CHG_ADDR "$1" b) || return 1
	new=$(((old & ~$2) | $3))
	i2cset -y $CHG_BUS $CHG_ADDR "$1" $new b
}

vbus_is_on() {
	[ $(($(i2cget -y $CHG_BUS $CHG_ADDR $REG_CNTL b) & 7)) = "$OP_MODE_USB_OTG" ]
}

# Why the port must not be powered now, or nothing.
power_refusal() {
	local status
	if [ ! -r "$FUELGAUGE/status" ]; then
		echo "the fuel gauge is missing, so a charger cannot be ruled out"
		return
	fi
	status=$(cat "$FUELGAUGE/status")
	if [ "$status" != Discharging ]; then
		echo "the fuel gauge says $status: something is powering the port"
		return
	fi
	if host_attached; then
		echo "a USB host has the gadget configured"
	fi
}

host_attached() {
	local state
	for state in "$SYSFS"/class/udc/*/state; do
		[ -r "$state" ] || continue
		[ "$(cat "$state")" = configured ] && return 0
	done
	return 1
}

# Real devices are 1-1, 1-1.2, ...; usb1 and 1-0:1.0 are the root hub itself.
usb_devices() {
	local n=0 d
	for d in "$SYSFS"/bus/usb/devices/*; do
		case ${d##*/} in
		[0-9]*-[0-9]*:*) ;;
		[0-9]*-[0-9]*) n=$((n + 1)) ;;
		esac
	done
	echo "$n"
}

role_file() {
	local r
	for r in "$SYSFS"/class/usb_role/*/role; do
		[ -e "$r" ] && {
			echo "$r"
			return
		}
	done
	die "no USB role switch (dr_mode otg + usb-role-switch, and dwc3 userspace control)"
}

role() { cat "$(role_file)"; }

vbus_on() {
	local why
	why=$(power_refusal)
	[ -z "$why" ] || {
		echo "tabs6-usb-role: not powering the port: $why" >&2
		return 1
	}
	rmw $REG_FLEDCNTL6 0x0f $BST_OUT_5100MV
	rmw $REG_CHGCNTL6 0x0c $((OTG_CURRENT_900MA << 2))
	rmw $REG_CNTL 0x07 $OP_MODE_USB_OTG
}

vbus_off() {
	rmw $REG_CNTL 0x07 $OP_MODE_CHG_ON
	rmw $REG_FLEDCNTL6 0x0f $BST_OUT_4500MV
}

to_host() {
	vbus_on || return 1
	echo host >"$(role_file)"
	log "role host, VBUS on"
}

to_device() {
	local r
	r=$(role_file)
	[ "$(cat "$r")" = device ] || echo device >"$r"
	vbus_off
	log "role device, VBUS off"
	# The UDC goes away in host role and the gadget is not bound again on
	# its own when it returns. The unit binds it (or keeps a binding).
	if [ -n "$GADGET_UNIT" ]; then
		systemctl restart --no-block "$GADGET_UNIT"
	fi
}

# Every boot starts as the gadget with the boost off. The role switch
# registers when dwc3 probes, which can be late; the gadget unit runs after
# this. Missing the switch is logged, never fatal.
boot_role() {
	local r=""
	for _ in $(seq 1 $((WAIT_S * 5))); do
		for r in "$SYSFS"/class/usb_role/*/role; do [ -e "$r" ] && break 2; done
		r=""
		sleep 0.2
	done
	if [ -n "$r" ]; then
		[ "$(cat "$r")" = device ] || echo device >"$r"
		log "role device"
	else
		log "no USB role switch after ${WAIT_S} s; leaving the role alone"
	fi
}

# The SM5705 keeps its registers across a warm reboot, so a reboot from host
# role would otherwise keep sourcing VBUS. Its I2C bus can probe late; this
# runs beside the gadget, not before it.
boot_vbus() {
	for _ in $(seq 1 $((WAIT_S * 5))); do
		[ -e "$I2C_DEV" ] && break
		sleep 0.2
	done
	if [ -e "$I2C_DEV" ] && vbus_off; then
		log "VBUS off"
	else
		log "could not reach the SM5705 on $I2C_DEV; VBUS left as it is"
	fi
}

status() {
	local v=off
	vbus_is_on && v=on
	echo "role $(role), VBUS $v, USB devices $(usb_devices)"
	if host_attached; then echo "a USB host has the gadget configured"; fi
	[ ! -r "$FUELGAUGE/status" ] || echo "fuel gauge: $(cat "$FUELGAUGE/status")"
}

case "${1:-status}" in
status) status ;;
host) to_host ;;
device) to_device ;;
boot-role) boot_role ;;
boot-vbus) boot_vbus ;;
*)
	echo "usage: tabs6-usb-role [status|host|device|boot-role|boot-vbus]" >&2
	exit 2
	;;
esac

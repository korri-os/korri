# tabs6-usb-role: turn the Tab S6 USB-C port between the gadget lifeline
# (device role) and a USB host that powers the port (a gamepad, keyboard, hub).
#
#   tabs6-usb-role [status]   the role, VBUS and attached USB devices
#   tabs6-usb-role host       power the port, then become a USB host
#   tabs6-usb-role sink-host  automatic-only host data role, no register writes
#   tabs6-usb-role device     become the gadget again, VBUS off, rebind the gadget
#   tabs6-usb-role boot-role  device role, before the gadget binds (boot)
#   tabs6-usb-role boot-vbus  VBUS off (boot)
#
# Ported from the reference port's tools/usb-role (docs/USB_HOST.md). The
# port is dual-role in the device tree (dr_mode = "otg", usb-role-switch) and
# dwc3 lets userspace drive the switch. The fixed read-only s2mm005 checks
# in tabs6-typec-auto provide permission; this helper owns every effect.
# A bus-powered device gets no power from
# the role switch alone: VBUS comes from the SM5705 charger's boost, which has
# no kernel driver, so it is set over I2C here. Register map from the vendor
# driver, cross-checked against the live part by the port (USB_HOST.md 2):
#
#   0x0c CNTL       bits 2:0   operation mode: 5 = CHG_ON, 7 = USB_OTG
#   0x14 CHGCNTL6   bits 3:2   OTG current limit: 2 = 900 mA
#   0x20 FLEDCNTL6  bits 3:0   boost output: 5 = 4500 mV, 0xb = 5100 mV
#
# VBUS is never sourced while anything else may be powering the port. The
# manual host path still requires Discharging. The automatic VBUS-on path
# remains blocked in the daemon. The separate sink-host path only selects a
# data role while Charging with CNTL mode 5, and never writes registers.
# Every host transition requires readable status and an unconfigured UDC.
#
# The test (usb-role.test.py) runs this file against a temporary sysfs tree.
# Paths and timings therefore come from the environment:
#   TABS6_SYSFS                    sysfs root (/sys)
#   TABS6_USB_GADGET_UNIT          unit that binds the gadget; restarted after
#                                  the port returns to device role
#   TABS6_I2C_DEV                  test override for the resolved device node
#   TABS6_USB_ROLE_WAIT_S          boot-*: how long to wait for the switch and
#                                  the I2C bus (20)
set -eu

SYSFS=${TABS6_SYSFS:-/sys}
GADGET_UNIT=${TABS6_USB_GADGET_UNIT:-}
I2C_DEV=${TABS6_I2C_DEV:-}
CHECKER=${TABS6_TYPEC_AUTO:-tabs6-typec-auto}
LOCK=${TABS6_USB_ROLE_LOCK:-/run/lock/tabs6-usb-role.lock}
WAIT_S=${TABS6_USB_ROLE_WAIT_S:-20}

CHG_BUS=""
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

resolve_bus() {
	if [ -z "$CHG_BUS" ]; then
		CHG_BUS=$("$CHECKER" --sm-bus) || return 1
		[[ "$CHG_BUS" =~ ^[0-9]+$ ]] || return 1
	fi
}

get_reg() {
	local value
	resolve_bus || return 1
	value=$(timeout --signal=KILL 0.2s i2cget -y "$CHG_BUS" $CHG_ADDR "$1" b) || return 1
	[[ "$value" =~ ^0x[[:xdigit:]]{2}$ ]] || return 1
	echo "$value"
}

rmw() { # register, mask, value
	local old new
	resolve_bus || return 1
	old=$(get_reg "$1") || return 1
	new=$(((old & ~$2) | $3))
	timeout --signal=KILL 0.2s i2cset -y "$CHG_BUS" $CHG_ADDR "$1" "$new" b || return 1
}

vbus_is_on() {
	local mode
	mode=$(get_reg $REG_CNTL) || return 1
	[ "$((mode & 7))" = "$OP_MODE_USB_OTG" ]
}

# Why the port must not be powered now, or nothing.
power_refusal() {
	local status
	if [ ! -r "$FUELGAUGE/status" ]; then
		echo "the fuel gauge is missing, so a charger cannot be ruled out"
		return
	fi
	status=$(cat "$FUELGAUGE/status") || { echo "fuel gauge read failed"; return; }
	if [ "$status" = Charging ] || { [ "${TABS6_AUTOMATIC:-0}" != 1 ] && [ "$status" != Discharging ]; }; then
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
	local why mode
	why=$(power_refusal) || return 1
	[ -z "$why" ] || {
		echo "tabs6-usb-role: not powering the port: $why" >&2
		return 1
	}
	# Automatic callers have already qualified three samples. Manual callers
	# must qualify too, and can never bypass the fixed read-only checks.
	if [ "${TABS6_AUTOMATIC:-0}" != 1 ]; then
		for _ in 1 2; do
			"$CHECKER" --check-host || return 1
			sleep 0.3
		done
	fi
	"$CHECKER" --check-host || return 1
	rmw $REG_FLEDCNTL6 0x0f $BST_OUT_5100MV || return 1
	rmw $REG_CHGCNTL6 0x0c $((OTG_CURRENT_900MA << 2)) || return 1
	# Configuration is not permission: recheck immediately before CNTL.
	"$CHECKER" --check-host || return 1
	why=$(power_refusal) || return 1
	[ -z "$why" ] || return 1
	rmw $REG_CNTL 0x07 $OP_MODE_USB_OTG || return 1
	mode=$(get_reg $REG_CNTL) || return 1
	[ "$((mode & 7))" = "$OP_MODE_USB_OTG" ]
}

vbus_off() {
	local mode
	for _ in 1 2 3; do
		if rmw $REG_CNTL 0x07 $OP_MODE_CHG_ON; then
			if mode=$(get_reg $REG_CNTL) && [ "$((mode & 7))" = "$OP_MODE_CHG_ON" ]; then
				rmw $REG_FLEDCNTL6 0x0f $BST_OUT_4500MV || return 1
				return 0
			fi
		fi
	done
	echo "tabs6-usb-role: ERROR: VBUS off not verified" >&2
	return 1
}

to_host() {
	local r
	if vbus_on && r=$(role_file) && echo host >"$r"; then
		log "role host, VBUS on"
	else
		# No retry of a partially failed enable, even inside an if/function.
		to_device || true
		return 1
	fi
}

# Only the data role changes. Both fresh interlocks include CNTL mode 5;
# this path never writes charger registers, even on rollback.
to_sink_host() {
	local r
	[ "${TABS6_AUTOMATIC:-0}" = 1 ] || die "sink-host is automatic-only"
	if ! host_attached && "$CHECKER" --check-sink-host && r=$(role_file) && echo host >"$r" && "$CHECKER" --check-sink-host; then
		log "role host, sinking power, CNTL mode 5 verified (no register writes)"
	else
		log "sink-host interlock/readback failed; returning to device role"
		data_device || true
		return 1
	fi
}

data_device() {
	local r failed=0
	if r=$(role_file); then
		if [ "$(cat "$r")" != device ]; then echo device >"$r" || failed=1; fi
	else
		failed=1
	fi
	if [ -n "$GADGET_UNIT" ]; then
		systemctl restart --no-block "$GADGET_UNIT" || failed=1
	fi
	return "$failed"
}

to_device() {
	local failed=0
	# Always request off first, but still select device if I2C is unavailable.
	vbus_off || failed=1
	data_device || failed=1
	if [ "$failed" = 0 ]; then log "role device, VBUS off (readback verified)"; fi
	return "$failed"
}

# InputPlumber 0.75.2 can retain a dead evdev source across Kishi power loss.
# Retire that provider while the host controller is absent, before any new
# qualification can enumerate the Kishi. Manual role commands stay unchanged.
to_sink_device() {
	local ready=0
	[ "${TABS6_AUTOMATIC:-0}" = 1 ] || die "sink-device is automatic-only"
	to_device || return 1
	for _ in $(seq 1 30); do
		if [ "$(role)" = device ] && [ "$(usb_devices)" = 0 ]; then
			ready=1
			break
		fi
		sleep 0.1
	done
	[ "$ready" = 1 ] || { log "host controller teardown not verified; staying device"; return 1; }
	if timeout --signal=KILL 10s systemctl restart inputplumber.service korri-inputd.service; then
		log "Kishi input providers restarted with USB absent; staying device"
	else
		log "Kishi input provider teardown failed; staying device"
		return 1
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
		if resolve_bus; then
			I2C_DEV=${TABS6_I2C_DEV:-/dev/i2c-$CHG_BUS}
			[ -e "$I2C_DEV" ] && break
		fi
		sleep 0.2
	done
	if [ -n "$I2C_DEV" ] && [ -e "$I2C_DEV" ] && vbus_off; then
		log "VBUS off"
	else
		log "could not reach the SM5705 on $I2C_DEV; VBUS left as it is"
	fi
}

status() {
	local v=off
	if vbus_is_on; then v=on; else
		get_reg $REG_CNTL >/dev/null || return 1
	fi
	echo "role $(role), VBUS $v, USB devices $(usb_devices)"
	if host_attached; then echo "a USB host has the gadget configured"; fi
	[ ! -r "$FUELGAUGE/status" ] || echo "fuel gauge: $(cat "$FUELGAUGE/status")"
}

# A manual choice stops automatic policy until explicitly restarted. Stop
# before locking: ExecStopPost uses the same serialized off/device path.
case "${1:-status}" in
host|device)
	if [ "${TABS6_AUTOMATIC:-0}" != 1 ]; then
		systemctl stop tabs6-typec-auto.service
	fi
	;;
esac
case "${1:-status}" in
host|sink-host|sink-device|device|boot-vbus)
	# boot-vbus can precede tmpfiles, which normally creates /run/lock.
	mkdir -p "${LOCK%/*}"
	exec 9>"$LOCK"
	flock -x 9
	;;
esac

case "${1:-status}" in
status) status ;;
host) to_host ;;
sink-host) to_sink_host ;;
sink-device) to_sink_device ;;
device) to_device ;;
boot-role) boot_role ;;
boot-vbus) boot_vbus ;;
*)
	echo "usage: tabs6-usb-role [status|host|sink-host|sink-device|device|boot-role|boot-vbus]" >&2
	exit 2
	;;
esac

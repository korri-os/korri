# Tab S6 USB-C helper. The product kernel owns the data role and the battery
# boost (s2mm005 policy, typec-kernel.md); this script never enables boost.
#
#   tabs6-usb-role [status]   role, VBUS, attached USB devices
#   tabs6-usb-role gadget-stop  gadget stop when the kernel selects host
#   tabs6-usb-role detached     gadget restart and input provider restart on device
#   tabs6-usb-role boot-role  device role at sysinit, before the late module
#   tabs6-usb-role boot-vbus  boost off at boot (SM5705 keeps registers across
#                             a warm reboot)
set -eu

SYSFS=${TABS6_SYSFS:-/sys}
GADGET_UNIT=${TABS6_USB_GADGET_UNIT:-}
I2C_DEV=${TABS6_I2C_DEV:-}
LOCK=${TABS6_USB_ROLE_LOCK:-/run/lock/tabs6-usb-role.lock}
WAIT_S=${TABS6_USB_ROLE_WAIT_S:-20}

CHG_BUS=""
CHG_ADDR=0x49
REG_CNTL=0x0c
REG_FLEDCNTL6=0x20

OP_MODE_CHG_ON=5
OP_MODE_USB_OTG=7
BST_OUT_4500MV=0x05

FUELGAUGE=$SYSFS/class/power_supply/sm5705-fuelgauge

log() { echo "tabs6-usb-role: $*"; }
die() {
	echo "tabs6-usb-role: $*" >&2
	exit 1
}

# The SM5705 adapter by its OF controller (i2c@890000), never by number.
resolve_bus() {
	local a found=""
	[ -z "$CHG_BUS" ] || return 0
	for a in "$SYSFS"/bus/i2c/devices/i2c-*; do
		[ "$(basename "$(readlink -f "$a/of_node" 2>/dev/null)")" = i2c@890000 ] || continue
		[ -z "$found" ] || return 1
		found=${a##*/i2c-}
	done
	[[ "$found" =~ ^[0-9]+$ ]] || return 1
	CHG_BUS=$found
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

# No role or chip writes. Triggered only by the read-only role watcher when
# the kernel returns the port to device (charger unplug, boost revoke, driver
# unbind). Rebinds the gadget lifeline and, once the host controller is torn
# down, restarts InputPlumber and inputd: a source re-added into a surviving
# composite can leave input dead (case B, reproduced 2026-11-25; see
# typec-kernel.md). Workaround until that is fixed and proven.
detached() {
	local ready=0
	[ -z "$GADGET_UNIT" ] || systemctl restart --no-block "$GADGET_UNIT"
	for _ in $(seq 1 30); do
		if [ "$(role)" = device ] && [ "$(usb_devices)" = 0 ]; then
			ready=1
			break
		fi
		sleep 0.1
	done
	[ "$ready" = 1 ] || { log "host controller teardown not verified; providers kept"; return 1; }
	if timeout --signal=KILL 10s systemctl restart inputplumber.service korri-inputd.service; then
		log "input providers restarted after kernel detach (no role/chip writes)"
	else
		log "input provider restart failed"
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

# The kernel policy's I2C client, by its OF compatible.
kernel_client() {
	local client found=""
	for client in "$SYSFS"/bus/i2c/devices/*; do
		[ -r "$client/of_node/compatible" ] || continue
		if tr '\0' '\n' <"$client/of_node/compatible" | grep -qx 'samsung,s2mm005-read-only'; then
			[ -z "$found" ] || die "multiple s2mm005 clients"
			found=$client
		fi
	done
	echo "$found"
}

case "${1:-status}" in
boot-vbus)
	mkdir -p "${LOCK%/*}"
	exec 9>"$LOCK"
	flock -x 9
	;;
esac

case "${1:-status}" in
status) status ;;
detached) detached ;;
gadget-stop)
	# Host can qualify before systemd starts the gadget. Cancel its late-UDC
	# wait without changing the shared product gadget or the kernel's role.
	if [ -n "$GADGET_UNIT" ]; then
		# A pending oneshot killed by a deliberate stop can retain Result=signal.
		# Wait for that job before clearing its failure, never before it ends.
		timeout --signal=KILL 3s systemctl stop "$GADGET_UNIT" || exit 1
		systemctl reset-failed "$GADGET_UNIT"
	fi
	;;
boot-role)
	# Runs at sysinit, before the late module loads. Never write once the
	# kernel driver is bound: from then on it owns the role.
	client=$(kernel_client) || exit 1
	[ -z "$client" ] || [ ! -e "$client/driver" ] || die "kernel owns boot data role"
	boot_role
	;;
boot-vbus) boot_vbus ;;
*)
	echo "usage: tabs6-usb-role [status|detached|gadget-stop|boot-role|boot-vbus]" >&2
	exit 2
	;;
esac

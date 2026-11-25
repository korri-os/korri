# Operator-only VBUS/role helper and kernel-role detach workaround.
# The product kernel owns the role. host/device are refused while its
# read-only s2mm005 driver is bound. Manual commands unbind it first.
# Chip writes exist ONLY in manual host/device and the unchanged boot-vbus.
# Register grounding and operator safety: typec.md and typec-phase-b.md.
set -eu

SYSFS=${TABS6_SYSFS:-/sys}
GADGET_UNIT=${TABS6_USB_GADGET_UNIT:-}
I2C_DEV=${TABS6_I2C_DEV:-}
CHECKER=${TABS6_TYPEC_MANUAL:-tabs6-typec-manual}
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
	local why mode
	why=$(power_refusal) || return 1
	[ -z "$why" ] || {
		echo "tabs6-usb-role: not powering the port: $why" >&2
		return 1
	}
	# Manual callers must qualify three fresh source samples.
	for _ in 1 2; do
		"$CHECKER" --check-host || return 1
		sleep 0.3
	done
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

# No role or chip writes. Triggered only by the read-only role watcher.
# Wait for the kernel's host-controller teardown before retiring providers.
detached() {
	local ready=0
	for _ in $(seq 1 30); do
		if [ "$(role)" = device ] && [ "$(usb_devices)" = 0 ]; then
			ready=1
			break
		fi
		sleep 0.1
	done
	[ "$ready" = 1 ] || { log "host controller teardown not verified"; return 1; }
	if [ -n "$GADGET_UNIT" ]; then
		systemctl restart --no-block "$GADGET_UNIT" || return 1
	fi
	if timeout --signal=KILL 10s systemctl restart inputplumber.service korri-inputd.service; then
		log "Kishi input providers restarted after kernel detach (no role/chip writes)"
	else
		log "Kishi input provider teardown failed"
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

# The client remains registered after unbind. Resolve its existing OF
# compatible, not an adapter number or a new persisted override flag.
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

kernel_yield() {
	local client
	client=$(kernel_client) || return 1
	[ -n "$client" ] || return 0
	# The driver is a module loaded late in boot. A manual override before
	# that load would be taken over by the probe; refuse until it is done.
	systemctl is-active --quiet tabs6-typec-kernel-policy.service ||
		die "kernel USB-C policy is not loaded yet; retry after boot"
	# Stop only the workaround, not policy. Driver remove synchronously cancels
	# sampling and requests DEVICE before it releases the role switch.
	systemctl stop tabs6-typec-role-watch.service || return 1
	if [ -e "$client/driver" ]; then
		echo "${client##*/}" >"$client/driver/unbind" || return 1
		log "kernel policy unbound for manual operator override"
	fi
}

kernel_resume() {
	local client
	client=$(kernel_client) || return 1
	[ -n "$client" ] || return 0
	if [ ! -e "$client/driver" ]; then
		if [ -e "$SYSFS/bus/i2c/drivers/s2mm005-read-only/bind" ]; then
			echo "${client##*/}" >"$SYSFS/bus/i2c/drivers/s2mm005-read-only/bind" || return 1
		else
			modprobe s2mm005 || return 1
		fi
		log "kernel policy rebound; qualification starts from zero"
	fi
	systemctl start tabs6-typec-role-watch.service
}

case "${1:-status}" in
host|device|boot-vbus)
	mkdir -p "${LOCK%/*}"
	exec 9>"$LOCK"
	flock -x 9
	;;
esac

case "${1:-status}" in
status) status ;;
host)
	kernel_yield || die "could not release kernel role owner"
	if ! to_host; then
		log "manual host failed; run device to verify boost-off and resume kernel policy"
		exit 1
	fi
	;;
device)
	kernel_yield || die "could not release kernel role owner"
	to_device && kernel_resume
	;;
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
	[ -z "$(kernel_client)" ] || die "kernel owns boot data role"
	boot_role
	;;
boot-vbus) boot_vbus ;;
*)
	echo "usage: tabs6-usb-role [status|host|device|detached|gadget-stop|boot-role|boot-vbus]" >&2
	exit 2
	;;
esac

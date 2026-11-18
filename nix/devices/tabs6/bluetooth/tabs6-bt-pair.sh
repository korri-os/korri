# tabs6-bt-pair: pair, trust and connect a Bluetooth controller, once.
#
#   tabs6-bt-pair            scan 20 s; pair the one controller in pairing mode
#   tabs6-bt-pair AA:BB:..   pair that address
#
# Run as root over SSH with the controller in pairing mode. Trusted, the
# controller reconnects by itself whenever it is switched on; BlueZ needs no
# further step. Korri has no pairing screen yet, so this is the first-time
# path. It only drives bluetoothctl; nothing here is specific to a model.
SCAN_S=${TABS6_BT_SCAN_S:-20}

bluetoothctl power on >/dev/null

scan() {
	# Discovery stays on while pairing: BlueZ forgets an unpaired device
	# soon after discovery stops.
	bluetoothctl --timeout $((SCAN_S + 60)) scan on >/dev/null &
	scanner=$!
	trap 'kill "$scanner" 2>/dev/null || true' EXIT
}

addr=${1:-}
scan
if [ -z "$addr" ]; then
	echo "scanning ${SCAN_S} s for a controller in pairing mode..."
	sleep "$SCAN_S"
	found=()
	while read -r _ mac _; do
		info=$(bluetoothctl info "$mac" || true)
		if grep -q "Icon: input-gaming" <<<"$info"; then
			found+=("$mac")
		fi
	done < <(bluetoothctl devices)
	if [ "${#found[@]}" -ne 1 ]; then
		echo "found ${#found[@]} controllers in pairing mode. Devices seen:" >&2
		bluetoothctl devices >&2
		echo "run again with the controller's address: tabs6-bt-pair AA:BB:CC:DD:EE:FF" >&2
		exit 1
	fi
	addr=${found[0]}
fi

echo "pairing $addr"
bluetoothctl --agent NoInputNoOutput pair "$addr"
bluetoothctl trust "$addr"
bluetoothctl connect "$addr"
info=$(bluetoothctl info "$addr")
grep -E "Name|Paired|Trusted|Connected" <<<"$info"

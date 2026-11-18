# Apply the efs Wi-Fi address to wlan0 after ath10k_snoc creates it.
#
# The .link file from tabs6-wifi-addr is not enough on its own: wlan0 appears
# about 5 s after modprobe, when the WLAN firmware is up, and udev then fails
# with "Could not set MACAddress=... Device or resource busy". Setting the
# address with the interface down works. This runs as ExecStartPost of
# tabs6-wifi, which iwd is ordered after, so iwd only ever sees the fixed
# address.
LINK=/run/systemd/network/10-tabs6-wlan.link
[ -f "$LINK" ] || { echo "no $LINK; keeping the driver's address"; exit 0; }
ADDR=$(sed -n 's/^MACAddress=//p' "$LINK")
[ -n "$ADDR" ] || exit 0

for _ in $(seq 1 60); do
	[ -e /sys/class/net/wlan0 ] && break
	sleep 1
done
[ -e /sys/class/net/wlan0 ] || { echo "wlan0 did not appear within 60 s"; exit 0; }

if [ "$(cat /sys/class/net/wlan0/address)" != "$ADDR" ]; then
	ip link set wlan0 down
	ip link set wlan0 address "$ADDR"
fi
echo "wlan0 address $(cat /sys/class/net/wlan0/address)"

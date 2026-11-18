# Give wlan0 the tablet's own Wi-Fi address before ath10k_snoc loads.
#
# The WCN3990 has no address the driver can read ("invalid MAC address;
# choosing random"), so wlan0 had a new address every boot and the router
# handed out a new lease each time. Samsung keeps the per-device address in
# /efs/wifi/.mac.info. efs is device identity and must never be written, so it
# is not mounted: debugfs opens the partition read-only (no -w) and prints the
# one file, as tabs6-bt-addr does.
#
# The address goes into a runtime .link file under /run (never onto disk), so
# udev sets it when the interface is created, before networkd or iwd see it.
EFS=/dev/disk/by-partlabel/efs
LINK=/run/systemd/network/10-tabs6-wlan.link

for _ in $(seq 1 30); do
	[ -b "$EFS" ] && break
	sleep 1
done
[ -b "$EFS" ] || { echo "no $EFS; keeping the driver's random address"; exit 0; }

ADDR=$(debugfs -R "cat /wifi/.mac.info" "$EFS" 2>/dev/null | tr -d '[:space:]' | tr 'A-F' 'a-f')
case "$ADDR" in
[0-9a-f][0-9a-f]:[0-9a-f][0-9a-f]:[0-9a-f][0-9a-f]:[0-9a-f][0-9a-f]:[0-9a-f][0-9a-f]:[0-9a-f][0-9a-f]) ;;
*) echo "efs has no usable wifi/.mac.info; keeping the driver's random address"; exit 0 ;;
esac

mkdir -p "${LINK%/*}"
cat >"$LINK" <<EOF
[Match]
Type=wlan
Driver=ath10k_snoc

[Link]
NamePolicy=keep kernel
MACAddressPolicy=none
MACAddress=$ADDR
EOF
udevadm control --reload
echo "wlan0 address from efs, set by $LINK"

# Give the WCN3990 its public Bluetooth address (BLUETOOTH.md section 4).
#
# The controller has none of its own and BlueZ keeps it "unconfigured" until it
# gets one. Samsung stores the per-device address in /efs/bluetooth/bt_addr.
# efs is device identity and must never be written, so it is not mounted at
# all: debugfs opens the partition read-only (no -w) and prints the one file.
# The address is never written anywhere on disk.
EFS=/dev/disk/by-partlabel/efs

for _ in $(seq 1 30); do
	[ -b "$EFS" ] && [ -e /sys/class/bluetooth/hci0 ] && break
	sleep 1
done
[ -b "$EFS" ] || { echo "no $EFS"; exit 0; }
[ -e /sys/class/bluetooth/hci0 ] || { echo "no hci0"; exit 0; }
# hci0 shows in sysfs while hci_qca still loads firmware; the mgmt index only
# answers after setup, and public-addr before that fails with Invalid Index.
for _ in $(seq 1 30); do
	# btmgmt exits 0 even on Invalid Index, so look for the info line.
	btmgmt --index 0 info 2>/dev/null | grep "addr " >/dev/null && break
	sleep 1
done

ADDR=$(debugfs -R "cat /bluetooth/bt_addr" "$EFS" 2>/dev/null | tr -d '[:space:]')
case "$ADDR" in
[0-9A-Fa-f][0-9A-Fa-f]:[0-9A-Fa-f][0-9A-Fa-f]:*) ;;
*) echo "efs has no usable bluetooth/bt_addr"; exit 1 ;;
esac

if btmgmt --index 0 info 2>/dev/null | grep -qi "addr $ADDR"; then
	echo "already configured"
	exit 0
fi
btmgmt --index 0 public-addr "$ADDR"

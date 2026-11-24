#!/usr/bin/env nix-shell
#!nix-shell -i bash -p bluez systemd procps gnugrep coreutils findutils
# Read-only: GameSir and Bluetooth state on the Korri tablet.
#   ssh -p 2222 root@tabs6.local bash -s < nix/devices/tabs6/scripts/gamesir-status.sh
export PATH=/run/current-system/sw/bin:$PATH
MAC=${1:-A0:5A:59:3F:06:42}
echo "== bluetoothd binary"
readlink -f "/proc/$(pidof bluetoothd)/exe"
echo "== device"
timeout 5 bluetoothctl info "$MAC" | grep -E "Name|Paired|Bonded|Trusted|Connected"
echo "== input devices"
grep -B1 -A5 -iE "gamesir|v3537|Vendor=3537" /proc/bus/input/devices | grep -E "N:|H:|I:"
echo "== uhid"
ls /dev/uhid
find /sys/bus/hid/devices/ -mindepth 1 -maxdepth 1 -printf '%f\n' | head
echo "== bluetoothd log"
journalctl -b -u bluetooth -o short-monotonic --no-pager | grep -v "journal file" | tail -12

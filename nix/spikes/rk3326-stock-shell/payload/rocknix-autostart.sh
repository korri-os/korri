#!/bin/sh
# ROCKNIX's /usr/bin/autostart runs every file in /storage/.config/autostart/
# as root at every boot, with no input from anyone. On a board with no keyboard, no network, and no
# USB data lines, that is the only hand that can type.
#
# Job: get our kernel's crash log off this device. Our image panics a second
# into boot and writes its console to the ramoops region at 0x110000. This
# ROCKNIX card reserves the same region, so on boot the records are either
# still in /sys/fs/pstore, or systemd-pstore has already moved them to
# /var/lib/systemd/pstore. Copy both to the FAT boot partition, which the
# card reader can read, and record what was found so an empty result still
# says why.
out=/flash/pstore
log=$out/README.txt

mount -o remount,rw /flash 2>/dev/null
mkdir -p "$out/live" "$out/archived"
{
  echo "=== $(date) ==="
  echo "kernel: $(uname -r)"
  echo "--- reserved-memory ---"
  ls /proc/device-tree/reserved-memory/ 2>&1
  echo "--- dmesg ramoops/pstore ---"
  dmesg | grep -iE "ramoops|pstore" 2>&1
  echo "--- /sys/fs/pstore ---"
  mount | grep pstore
  ls -la /sys/fs/pstore/ 2>&1
  echo "--- /var/lib/systemd/pstore ---"
  ls -laR /var/lib/systemd/pstore/ 2>&1
} > "$log" 2>&1

cp -a /sys/fs/pstore/. "$out/live/" 2>>"$log"
cp -a /var/lib/systemd/pstore/. "$out/archived/" 2>>"$log"
sync

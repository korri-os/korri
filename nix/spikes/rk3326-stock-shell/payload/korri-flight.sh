#!/bin/sh
# Stage-1 flight recorder for the R36T Max SD card.
#
# $1 is a stage label. Runs from the initrd, where the only tools are
# busybox and util-linux.
#
# The build puts the shared selector next to this script in the initrd.
# A missing label means no card log. It must never mean writing to another
# filesystem, especially the stock installation on internal eMMC.
. "$(dirname "$0")/boot-media.sh"
mounted=$(r36tmax_boot_partition) || {
  echo "flight recorder: no verified SD boot partition; skipping write" >&2
  exit 0
}
[ -b "$mounted" ] || exit 0
mkdir -p /flight
mount -t vfat "$mounted" /flight 2>/dev/null || exit 0

{
  echo "=== $1 $(cat /proc/sys/kernel/random/boot_id 2>/dev/null) via $mounted ==="
  cat /proc/uptime
  cat /proc/cmdline
  echo "--- block devices ---"
  cat /proc/partitions
  echo "--- by-label ---"
  ls -la /dev/disk/by-label/ 2>&1
  echo "--- mmc hosts ---"
  for h in /sys/class/mmc_host/*; do
    [ -d "$h" ] || continue
    echo "$(basename "$h"): $(ls "$h" | grep '^mmc' | tr '\n' ' ')"
  done
  echo "--- dmesg ---"
  dmesg || true
} >> /flight/flight.log 2>&1

mkdir -p /pstore
if mount -t pstore pstore /pstore 2>/dev/null; then
  for f in /pstore/*; do
    [ -e "$f" ] || continue
    {
      echo "=== pstore $(basename "$f") ==="
      cat "$f"
    } >> /flight/pstore.log 2>&1
  done
  umount /pstore
fi
sync
umount /flight

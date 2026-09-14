#!/bin/sh
# Stage-1 flight recorder for a board with no console.
#
# $1 is a stage label. Runs from the initrd, where the only tools are
# busybox and util-linux.
#
# It must not depend on the thing it diagnoses. A boot that cannot find the
# root label is exactly the boot that needs a log, and by-label is what it
# cannot find. So try the label, and failing that try every vfat partition
# the kernel can see, and write to the first one that mounts. Record which
# block devices exist at all, because that is the question a missing root
# actually asks.
mkdir -p /flight
mounted=
if mount -t vfat /dev/disk/by-label/NIXOS_BOOT /flight 2>/dev/null; then
  mounted=label
else
  for dev in /dev/mmcblk*p1 /dev/mmcblk*p* /dev/sd*1; do
    [ -b "$dev" ] || continue
    if mount -t vfat "$dev" /flight 2>/dev/null; then
      mounted=$dev
      break
    fi
  done
fi
[ -n "$mounted" ] || exit 0

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

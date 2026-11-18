#!/usr/bin/env bash
# tabs6-esp: write the systemd-boot ESP on the Tab S6 `cache` partition.
#
#   tabs6-esp stage <toplevel> <dir>   write a fresh ESP tree into <dir> (image build)
#   tabs6-esp install <toplevel>       make <toplevel> the default entry (device)
#   tabs6-esp mark-good                copy the booted system to the LAST KNOWN GOOD entry
#   tabs6-esp rollback                 make LAST KNOWN GOOD the default entry again
#   tabs6-esp status                   show both entries and free space
#
# Layout (all paths on the FAT):
#   /EFI/BOOT/BOOTAA64.EFI               systemd-boot
#   /loader/loader.conf                  default tabs6-current.conf
#   /loader/entries/tabs6-current.conf   /tabs6/current/{Image,gts6lwifi.dtb}
#   /loader/entries/tabs6-lastgood.conf  /tabs6/lastgood/{Image,gts6lwifi.dtb}
#
# Every entry carries init=<toplevel>/init, so the modules come with the closure
# and no /lib/modules snapshot is needed. Each entry's toplevel is held by a GC
# root, /nix/var/nix/gcroots/tabs6-{current,lastgood}.
#
# The ESP has 512 byte logical sectors on a 4096 byte UFS device, so it only
# mounts through a loop device (PORT.md). The trap always unmounts and detaches
# it: a stale loop on the ESP wedges shutdown. Only /dev/disk/by-partlabel/cache
# is ever opened, and it is never formatted.
set -euo pipefail

ESP_DEV=/dev/disk/by-partlabel/cache
GCROOTS=/nix/var/nix/gcroots
DTB_NAME=gts6lwifi.dtb

die() { echo "tabs6-esp: $*" >&2; exit 1; }

# write_tree <toplevel> <esp-root> <slot>
write_slot() {
	local top=$1 root=$2 slot=$3 title
	[ -e "$top/init" ] || die "$top is not a NixOS system"
	[ -e "$top/tabs6-dtb" ] || die "$top has no tabs6-dtb"
	mkdir -p "$root/tabs6/$slot" "$root/loader/entries"
	# Write under temporary names, then rename, so a failed copy never leaves
	# a half-written Image behind the entry.
	cp "$top/kernel" "$root/tabs6/$slot/Image.new"
	cp "$top/tabs6-dtb" "$root/tabs6/$slot/$DTB_NAME.new"
	case $slot in
	current) title="NixOS" ;;
	lastgood) title="NixOS LAST KNOWN GOOD" ;;
	esac
	cat > "$root/loader/entries/tabs6-$slot.conf.new" <<EOF
title $title
version $(basename "$top")
sort-key tabs6-$slot
linux /tabs6/$slot/Image
devicetree /tabs6/$slot/$DTB_NAME
options init=$top/init $(cat "$top/kernel-params")
EOF
	sync
	mv -f "$root/tabs6/$slot/Image.new" "$root/tabs6/$slot/Image"
	mv -f "$root/tabs6/$slot/$DTB_NAME.new" "$root/tabs6/$slot/$DTB_NAME"
	mv -f "$root/loader/entries/tabs6-$slot.conf.new" "$root/loader/entries/tabs6-$slot.conf"
	sync
}

write_loader() {
	local top=$1 root=$2
	mkdir -p "$root/EFI/BOOT" "$root/loader"
	cp "$top/tabs6-systemd-boot.efi" "$root/EFI/BOOT/BOOTAA64.EFI.new"
	mv -f "$root/EFI/BOOT/BOOTAA64.EFI.new" "$root/EFI/BOOT/BOOTAA64.EFI"
	# Hold volume-down for the menu; the timeout leaves a window without it.
	cat > "$root/loader/loader.conf" <<EOF
default tabs6-current.conf
timeout 3
editor no
console-mode keep
EOF
	sync
}

entry_toplevel() {
	local f=$1
	[ -f "$f" ] || return 1
	sed -n 's/^options init=\(\/nix\/store\/[^ ]*\)\/init.*/\1/p' "$f"
}

MNT=""
LOOP=""
cleanup() {
	if [ -n "$MNT" ] && mountpoint -q "$MNT"; then umount "$MNT"; fi
	if [ -n "$LOOP" ]; then losetup -d "$LOOP"; fi
	if [ -n "$MNT" ]; then rmdir "$MNT" 2>/dev/null || true; fi
}

mount_esp() {
	[ "$(id -u)" = 0 ] || die "run as root"
	[ -b "$ESP_DEV" ] || die "$ESP_DEV not found"
	trap cleanup EXIT INT TERM
	LOOP=$(losetup --find --show --sector-size 512 "$ESP_DEV")
	[ "$(blkid -o value -s TYPE "$LOOP" || true)" = vfat ] || die "$ESP_DEV is not a FAT ESP; refusing to touch it"
	MNT=$(mktemp -d /run/tabs6-esp.XXXXXX)
	mount -t vfat -o noatime "$LOOP" "$MNT"
}

gcroot() { ln -sfn "$2" "$GCROOTS/tabs6-$1"; }

cmd=${1:-status}
case $cmd in
stage)
	[ $# = 3 ] || die "usage: tabs6-esp stage <toplevel> <dir>"
	write_loader "$2" "$3"
	write_slot "$2" "$3" current
	;;
install)
	[ $# = 2 ] || die "usage: tabs6-esp install <toplevel>"
	top=$(readlink -f "$2")
	if [ "$(cat "$top/tabs6-kernel-kind" 2>/dev/null)" != device ]; then
		die "$top uses the placeholder kernel; it cannot boot the tablet"
	fi
	mount_esp
	old=$(entry_toplevel "$MNT/loader/entries/tabs6-current.conf" || true)
	booted=$(readlink -f /run/booted-system || true)
	# Seed the fallback once: the entry we are running from has survived a
	# reboot, which is the port's rule for a snapshot. An existing lastgood is
	# never replaced here; that is `mark-good`.
	if [ ! -f "$MNT/loader/entries/tabs6-lastgood.conf" ] && [ -n "$old" ] && [ "$old" = "$booted" ] && [ "$old" != "$top" ]; then
		write_slot "$old" "$MNT" lastgood
		gcroot lastgood "$old"
		echo "seeded LAST KNOWN GOOD with the booted system $old"
	fi
	write_loader "$top" "$MNT"
	write_slot "$top" "$MNT" current
	gcroot current "$top"
	echo "default entry: $top"
	;;
mark-good)
	booted=$(readlink -f /run/booted-system)
	mount_esp
	cur=$(entry_toplevel "$MNT/loader/entries/tabs6-current.conf" || true)
	[ "$cur" = "$booted" ] || die "the booted system is not the default entry; reboot into it first"
	write_slot "$booted" "$MNT" lastgood
	gcroot lastgood "$booted"
	echo "LAST KNOWN GOOD is now $booted"
	;;
rollback)
	mount_esp
	lg=$(entry_toplevel "$MNT/loader/entries/tabs6-lastgood.conf" || true)
	[ -n "$lg" ] && [ -e "$lg/init" ] || die "no LAST KNOWN GOOD entry"
	write_slot "$lg" "$MNT" current
	gcroot current "$lg"
	echo "default entry is LAST KNOWN GOOD again ($lg). Reboot."
	;;
status)
	mount_esp
	df -h "$MNT" | tail -1
	for slot in current lastgood; do
		printf '%-9s %s\n' "$slot" "$(entry_toplevel "$MNT/loader/entries/tabs6-$slot.conf" || echo none)"
	done
	echo "booted    $(readlink -f /run/booted-system)"
	;;
*)
	die "usage: tabs6-esp stage|install|mark-good|rollback|status"
	;;
esac

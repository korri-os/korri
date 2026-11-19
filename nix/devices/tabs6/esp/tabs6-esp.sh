#!/usr/bin/env bash
# tabs6-esp: write the systemd-boot ESP on the Tab S6 `cache` partition.
#
#   tabs6-esp stage <toplevel> <dir>   write a fresh ESP tree into <dir> (image build)
#   tabs6-esp install <toplevel>       make <toplevel> the default entry (device)
#   tabs6-esp trial                    exit 0 if this boot is an unblessed trial of the default entry
#   tabs6-esp bless                    keep the default entry: drop its boot counter
#   tabs6-esp mark-good                bless, then copy the booted system to the LAST KNOWN GOOD entry
#   tabs6-esp rollback                 make LAST KNOWN GOOD the default entry again
#   tabs6-esp status                   show both entries and free space
#   tabs6-esp fsck-once                boot once into an offline e2fsck of userdata
#   tabs6-esp fsck-done <log>          (tabs6-fsck-init) keep the log, restore the normal entry
#   tabs6-esp fsck-log                 print the last offline check's log
#
# Layout (all paths on the FAT):
#   /EFI/BOOT/BOOTAA64.EFI               systemd-boot
#   /loader/loader.conf                  preferred tabs6-current.conf, default tabs6-lastgood.conf
#   /loader/entries/tabs6-current[+L-D].conf   /tabs6/current/{Image,gts6lwifi.dtb}
#   /loader/entries/tabs6-lastgood.conf  /tabs6/lastgood/{Image,gts6lwifi.dtb}
#
# Boot counting (systemd-boot "Automatic Boot Assessment"): `install` writes
# the new default as tabs6-current+1-0.conf. systemd-boot renames it to
# tabs6-current+0-1.conf before it boots it; Mu's FAT driver on the UFS ESP
# does the rename (seen on the tablet, 2026-11-18). An entry with no tries
# left is "bad": `preferred` skips it and `default` names LAST KNOWN GOOD, so
# the next boot after a failed trial starts LAST KNOWN GOOD by itself. The
# booted system blesses the entry (tabs6-boot-good.service runs `bless`) once
# its own checks pass; `bless` renames it back to tabs6-current.conf.
# `rollback` and `stage` write uncounted entries: both systems are known good
# or have nothing to fall back to.
#
# Offline root check: `fsck-once` writes tabs6-fsck+1-0.conf, which boots the
# current kernel with root read-only and init=tabs6-fsck-init
# (tabs6-fsck-init.sh, in this package), and makes it `preferred` with tabs6-current.conf as
# `default`. The script runs e2fsck -fy, then `fsck-done` saves its log as
# /tabs6/fsck.log and writes the normal loader.conf back. If it never gets
# that far, its single try is spent and systemd-boot boots tabs6-current.
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

# The current entry's file, counted or not; empty when there is none.
current_entry() {
	local f
	for f in "$1"/loader/entries/tabs6-current.conf "$1"/loader/entries/tabs6-current+*.conf; do
		[ -f "$f" ] && { echo "$f"; return 0; }
	done
	return 0
}

# write_slot <toplevel> <esp-root> <slot> [tries]
# With tries, the entry is written with a systemd-boot boot counter.
write_slot() {
	local top=$1 root=$2 slot=$3 tries=${4:-} title name f
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
	name=tabs6-$slot.conf
	[ -z "$tries" ] || name=tabs6-$slot+$tries-0.conf
	# One entry per slot: drop the other counter states of this slot.
	for f in "$root/loader/entries/tabs6-$slot.conf" "$root/loader/entries/tabs6-$slot+"*.conf; do
		[ -f "$f" ] && [ "$(basename "$f")" != "$name" ] && rm -f "$f"
	done
	mv -f "$root/loader/entries/tabs6-$slot.conf.new" "$root/loader/entries/$name"
	sync
}

# write_loader_conf <esp-root> [preferred default]
write_loader_conf() {
	local root=$1 preferred=${2:-tabs6-current.conf} default=${3:-tabs6-lastgood.conf}
	mkdir -p "$root/loader"
	# Hold volume-down for the menu; the timeout leaves a window without it.
	# `preferred` skips an entry with no tries left; `default` then picks
	# LAST KNOWN GOOD (systemd-boot 261, config_select_default_entry).
	cat > "$root/loader/loader.conf.new" <<EOF
preferred $preferred
default $default
timeout 3
editor no
console-mode keep
EOF
	mv -f "$root/loader/loader.conf.new" "$root/loader/loader.conf"
	sync
}

write_loader() {
	local top=$1 root=$2
	mkdir -p "$root/EFI/BOOT"
	cp "$top/tabs6-systemd-boot.efi" "$root/EFI/BOOT/BOOTAA64.EFI.new"
	mv -f "$root/EFI/BOOT/BOOTAA64.EFI.new" "$root/EFI/BOOT/BOOTAA64.EFI"
	write_loader_conf "$root"
}

drop_fsck_entries() {
	local f
	for f in "$1"/loader/entries/tabs6-fsck.conf "$1"/loader/entries/tabs6-fsck+*.conf; do
		[ -f "$f" ] && rm -f "$f"
	done
	return 0
}

# Trial: the default entry still carries a boot counter and holds the booted system.
is_trial() {
	local f booted
	f=$(current_entry "$MNT")
	booted=$(readlink -f /run/booted-system)
	case $(basename "${f:-none}") in
	tabs6-current+*.conf) [ "$(entry_toplevel "$f")" = "$booted" ] ;;
	*) return 1 ;;
	esac
}

bless() {
	local f
	f=$(current_entry "$MNT")
	case $(basename "${f:-none}") in
	tabs6-current+*.conf)
		[ "$(entry_toplevel "$f")" = "$(readlink -f /run/booted-system)" ] || die "the booted system is not the default entry; not blessing it"
		mv -f "$f" "$MNT/loader/entries/tabs6-current.conf"
		sync
		echo "blessed: $(basename "$f") -> tabs6-current.conf"
		;;
	*) echo "nothing to bless: the default entry has no boot counter" ;;
	esac
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
	old=$(entry_toplevel "$(current_entry "$MNT")" || true)
	booted=$(readlink -f /run/booted-system || true)
	# Seed the fallback once: the entry we are running from has survived a
	# reboot, which is the port's rule for a snapshot. An existing lastgood is
	# never replaced here; that is `mark-good`.
	if [ ! -f "$MNT/loader/entries/tabs6-lastgood.conf" ] && [ -n "$old" ] && [ "$old" = "$booted" ] && [ "$old" != "$top" ]; then
		write_slot "$old" "$MNT" lastgood
		gcroot lastgood "$old"
		echo "seeded LAST KNOWN GOOD with the booted system $old"
	fi
	drop_fsck_entries "$MNT"
	write_loader "$top" "$MNT"
	# One trial boot: if it is not blessed, the next boot is LAST KNOWN GOOD.
	write_slot "$top" "$MNT" current 1
	gcroot current "$top"
	echo "default entry: $top (one trial boot)"
	;;
trial)
	mount_esp
	is_trial
	;;
bless)
	mount_esp
	bless
	;;
mark-good)
	booted=$(readlink -f /run/booted-system)
	mount_esp
	cur=$(entry_toplevel "$(current_entry "$MNT")" || true)
	[ "$cur" = "$booted" ] || die "the booted system is not the default entry; reboot into it first"
	bless
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
fsck-once)
	booted=$(readlink -f /run/booted-system)
	# tabs6-fsck-init ships next to this command (package.nix), so any
	# system that has this tabs6-esp can schedule the check.
	init=$(readlink -f "$(dirname "$0")/tabs6-fsck-init" || true)
	[ -x "$init" ] || die "no tabs6-fsck-init next to $0"
	mount_esp
	cur=$(current_entry "$MNT")
	# The fallback after a failed check is the current entry, so it must be
	# the booted, blessed system.
	[ "$(basename "${cur:-none}")" = tabs6-current.conf ] || die "the default entry is not blessed (${cur:-none}); bless or mark-good first"
	[ "$(entry_toplevel "$cur")" = "$booted" ] || die "the booted system is not the default entry"
	# The booted command line with root read-only and without the splash.
	params=""
	# kernel-params has no trailing newline, so read returns 1 at its end.
	read -ra words < "$booted/kernel-params" || true
	for p in "${words[@]}"; do
		case $p in
		rw) params="$params ro" ;;
		quiet | splash | loglevel=*) ;;
		*) params="$params $p" ;;
		esac
	done
	case "$params " in *" ro "*) ;; *) die "could not make root read-only in:$params" ;; esac
	drop_fsck_entries "$MNT"
	# panic=10: if PID 1 dies, the kernel reboots 10 s later.
	cat > "$MNT/loader/entries/tabs6-fsck+1-0.conf.new" <<EOF
title NixOS offline root check (one time)
version $(basename "$booted")
sort-key tabs6-fsck
linux /tabs6/current/Image
devicetree /tabs6/current/$DTB_NAME
options init=$init$params loglevel=4 panic=10
EOF
	mv -f "$MNT/loader/entries/tabs6-fsck+1-0.conf.new" "$MNT/loader/entries/tabs6-fsck+1-0.conf"
	rm -f "$MNT/tabs6/fsck.log"
	write_loader_conf "$MNT" tabs6-fsck.conf tabs6-current.conf
	echo "next boot: offline e2fsck of userdata, then $booted again. Reboot."
	;;
fsck-done)
	[ $# = 2 ] || die "usage: tabs6-esp fsck-done <log>"
	mount_esp
	mkdir -p "$MNT/tabs6"
	cp "$2" "$MNT/tabs6/fsck.log.new" && mv -f "$MNT/tabs6/fsck.log.new" "$MNT/tabs6/fsck.log"
	drop_fsck_entries "$MNT"
	write_loader_conf "$MNT"
	echo "normal boot entry restored"
	;;
fsck-log)
	mount_esp
	[ -f "$MNT/tabs6/fsck.log" ] || die "no offline check log"
	cat "$MNT/tabs6/fsck.log"
	;;
status)
	mount_esp
	df -h "$MNT" | tail -1
	cur=$(current_entry "$MNT")
	printf '%-9s %s (%s)\n' current "$(entry_toplevel "$cur" || echo none)" "$(basename "${cur:-none}")"
	printf '%-9s %s\n' lastgood "$(entry_toplevel "$MNT/loader/entries/tabs6-lastgood.conf" || echo none)"
	echo "booted    $(readlink -f /run/booted-system)"
	;;
*)
	die "usage: tabs6-esp stage|install|trial|bless|mark-good|rollback|status|fsck-once|fsck-done|fsck-log"
	;;
esac

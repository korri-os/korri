# tabs6-fsck-init: PID 1 of the one-time root check entry (`tabs6-esp
# fsck-once`). The kernel mounted userdata read-only (`ro` on that entry), so
# e2fsck can repair it before anything writes to it. No initrd, no systemd:
# this script mounts what it needs, checks, writes its log to the ESP,
# restores the normal boot entry and reboots.
#
# Every way out reboots: the end of this script, a deadline, and (if bash
# itself dies) the kernel's `panic=10` on that entry. The entry has one boot
# try, so after a failed run systemd-boot skips it and boots the normal entry.
#
# PATH is set by the wrapper (package.nix). Root stays read-only throughout:
# the log lives on a tmpfs and goes to the ESP, never to userdata.

reboot_now() {
	echo "tabs6-fsck: rebooting"
	sync
	echo s >/proc/sysrq-trigger
	sleep 2
	echo b >/proc/sysrq-trigger
	sleep 30
}

mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t tmpfs -o mode=0755 tmpfs /run
# Process substitution needs /dev/fd; without udev nothing makes it.
[ -e /dev/fd ] || ln -s /proc/self/fd /dev/fd
log=/run/tabs6-fsck.log
exec > >(tee -a "$log") 2>&1

# Whatever happens below, the tablet reboots within an hour.
(
	sleep 3600
	echo "tabs6-fsck: deadline reached"
	reboot_now
) &

echo "tabs6-fsck: $(date -u '+%F %T') boot $(cat /proc/sys/kernel/random/boot_id)"

# No udev: make the /dev/disk/by-partlabel links tabs6-esp opens.
partlabels() {
	local u name
	mkdir -p /dev/disk/by-partlabel
	for u in /sys/class/block/*/uevent; do
		name=$(sed -n 's/^PARTNAME=//p' "$u")
		[ -n "$name" ] && ln -sfn "../../$(basename "$(dirname "$u")")" "/dev/disk/by-partlabel/$name"
	done
	return 0
}

# The kernel finds userdata itself (rootwait); its node may lag the mount.
root=""
for _ in $(seq 1 30); do
	partlabels
	if [ -b /dev/disk/by-partlabel/userdata ] && [ -b /dev/disk/by-partlabel/cache ]; then
		root=$(readlink -f /dev/disk/by-partlabel/userdata)
		break
	fi
	sleep 1
done

summary() {
	tune2fs -l "$1" | grep -E '^(Filesystem state|FS Error count|Last error|First error|Mount count|Last checked)' || true
}

rc=none
if [ -z "$root" ] || [ ! -b "$root" ]; then
	echo "tabs6-fsck: no userdata partition"
elif ! grep -qE '^[^ ]+ / ext4 ro[ ,]' /proc/mounts; then
	echo "tabs6-fsck: / is not mounted read-only; not checking"
	grep ' / ' /proc/mounts
else
	# Same as tabs6-ufs-write-through: a write must reach flash before it
	# completes, because the UFS link resets the device about once a second.
	# Unit attentions from the resets fail single writes, hence the retries.
	d=/sys/class/scsi_disk/0:0:0:0
	for _ in $(seq 1 50); do
		[ "$(cat "$d/cache_type" 2>/dev/null)" = "write through" ] && break
		echo "write through" >"$d/cache_type" 2>/dev/null || sleep 0.1
	done
	echo "sda cache: $(cat "$d/cache_type" 2>/dev/null)"
	echo "== before ($root)"
	summary "$root"
	echo "== e2fsck -fy $root"
	e2fsck -fy "$root"
	rc=$?
	echo "== e2fsck exit $rc"
	sync
	echo "== after"
	summary "$root"
fi
echo "tabs6-fsck: result $rc"

# Restore the normal entry and keep the log, through tabs6-esp (the ESP is a
# separate partition; userdata is not written).
tabs6-esp fsck-done "$log" || echo "tabs6-fsck: fsck-done failed; the counted entry falls back by itself"
reboot_now

# deploy-over-ssh: update a running Tab S6 over the USB or Wi-Fi SSH link.
#
#   deploy-over-ssh [--switch] [--reboot] [--port N] [user@host]
#
# 1. copies the system closure to the tablet (nix copy over ssh-ng),
# 2. syncs /lib/firmware from the local device-firmware build,
# 3. on a tablet with no plugin receipts yet, installs the product's seeded
#    plugins (receipts, GC roots and their closures), as a fresh image has,
# 4. sets the system profile and runs switch-to-configuration boot (or switch),
#    whose boot loader hook (tabs6-esp install) writes the new default entry
#    onto the cache ESP through a loop device, with a cleanup trap. The LAST
#    KNOWN GOOD entry is kept; run `tabs6-esp mark-good` on the tablet after
#    the new system has survived a reboot.
# 5. selects the system's own Korri service bundle (korri-bundle-select
#    switch), as the device update procedures do (nix/devices/rgds/UPDATE.md):
#    system activation never changes the persistent bundle selection, so a
#    new system would otherwise keep running the old korrid. With --switch it
#    selects at once. With --reboot it waits for the new system to boot and
#    bless its trial entry, then selects; a boot that fell back to LAST KNOWN
#    GOOD keeps its bundle and the deploy fails. Without either it prints the
#    command to run after the reboot.
#
# It writes only the root filesystem and, through tabs6-esp, the cache ESP.
# It never opens any other partition and never flashes anything.
#
# Set by the wrapper: toplevel, bundle (empty on the console), firmware,
# placeholder, plugin_seed, sign_key, default_target, default_port.
#
# Signing: the Korri product requires signed paths (nix/device-cache), so
# its wrapper signs the closure with Zao's cache key, as
# nix/devices/system-update does, and copies with signatures checked. The
# console wrapper has no key and copies unsigned paths with --no-check-sigs,
# as the tabs6 repository did; root on the console is a trusted user.
mode=boot
reboot=no
target=$default_target
port=$default_port
while [ $# -gt 0 ]; do
	case $1 in
	--switch) mode=switch ;;
	--reboot) reboot=yes ;;
	--port)
		port=$2
		shift
		;;
	-h | --help)
		echo "usage: deploy-over-ssh [--switch] [--reboot] [--port N] [user@host]"
		echo "default: $default_target, port $default_port"
		exit 0
		;;
	-*)
		echo "unknown option $1" >&2
		exit 2
		;;
	*) target=$1 ;;
	esac
	shift
done

if [ "$placeholder" = true ]; then
	echo "deploy-over-ssh: this build uses the placeholder nixpkgs kernel, which cannot boot the tablet. Refusing." >&2
	exit 1
fi

ssh_opts=(-o ConnectTimeout=10 -o ServerAliveInterval=10 -p "$port")
export NIX_SSHOPTS="${ssh_opts[*]}"
on_tablet() {
	# shellcheck disable=SC2029 # expanded locally on purpose
	ssh "${ssh_opts[@]}" "$target" "$@"
}

# The plugin seed is needed only where the device has no receipts yet.
seed=no
if [ -s "$plugin_seed/seed.tar" ] && [ -n "$(tar -tf "$plugin_seed/seed.tar" | grep -m1 korri-plugin-host || true)" ]; then
	if on_tablet "test -e /var/lib/korri-plugin-host"; then
		echo "== the tablet already has plugin receipts; the plugin host keeps them"
	else
		seed=yes
	fi
fi
paths=("$toplevel")
[ "$seed" = no ] || paths+=("$plugin_seed")

copy_flags=()
if [ -n "$sign_key" ]; then
	[ -r "$sign_key" ] || {
		echo "deploy-over-ssh: no signing key at $sign_key (run on Zao)" >&2
		exit 1
	}
	echo "== signing ${paths[*]}"
	nix store sign --recursive --key-file "$sign_key" "${paths[@]}"
else
	copy_flags+=(--no-check-sigs)
fi

echo "== copying ${paths[*]} to $target"
nix copy "${copy_flags[@]}" --to "ssh-ng://$target" "${paths[@]}"

echo "== syncing /lib/firmware"
rsync -a --delete --checksum -e "ssh ${ssh_opts[*]}" "$firmware/lib/firmware/" "$target:/lib/firmware/"

if [ "$seed" = yes ]; then
	echo "== installing the seeded plugin receipts"
	on_tablet "test ! -e /var/lib/korri-plugin-host && tar -C / -xpf $plugin_seed/seed.tar --keep-old-files"
fi

select_cmd="/run/current-system/sw/bin/korri-bundle-select switch $bundle /run/current-system/sw/bin/systemctl"
select_bundle() {
	echo "== selecting the system's service bundle $bundle"
	on_tablet "$select_cmd"
	# The selector checks health; this checks it runs the bundle's korrid.
	on_tablet "test \"\$(readlink -f /nix/var/nix/gcroots/korri-bundle/active)\" = $bundle && test \"\$(readlink -f /proc/\$(systemctl show korrid -p MainPID --value)/exe)\" = \"\$(readlink -f $bundle/bin/korrid)\"" || {
		echo "deploy-over-ssh: korrid does not run the bundle's korrid" >&2
		exit 1
	}
	echo "korrid runs $(on_tablet "readlink -f $bundle/bin/korrid")"
}

old_boot=$(on_tablet "cat /proc/sys/kernel/random/boot_id")

echo "== installing ($mode)"
on_tablet "nix-env -p /nix/var/nix/profiles/system --set $toplevel && $toplevel/bin/switch-to-configuration $mode && $toplevel/sw/bin/tabs6-esp status"

if [ "$mode" = switch ] && [ -n "$bundle" ]; then
	select_bundle
fi

if [ "$reboot" = yes ]; then
	echo "== rebooting"
	on_tablet "systemctl reboot" || true
	[ -n "$bundle" ] || exit 0
	# The trial boot has tabs6.bootGood.timeout (300 s) to bless itself.
	echo "== waiting for the new system to boot and bless its entry"
	deadline=$((SECONDS + 600))
	booted=""
	while [ "$SECONDS" -lt "$deadline" ]; do
		sleep 10
		boot=$(on_tablet "cat /proc/sys/kernel/random/boot_id" 2>/dev/null) || continue
		[ "$boot" != "$old_boot" ] || continue
		booted=$(on_tablet "readlink -f /run/booted-system") || continue
		if [ "$booted" != "$toplevel" ]; then
			echo "deploy-over-ssh: the tablet booted $booted, not $toplevel (LAST KNOWN GOOD?); the bundle selection is unchanged" >&2
			exit 1
		fi
		# Blessed: tabs6-boot-good ran to a clean exit. (Not `tabs6-esp
		# trial`: a second ESP mount while the unit blesses could race.)
		state=$(on_tablet "systemctl show tabs6-boot-good -p ActiveState -p Result -p ExecMainExitTimestampMonotonic --value | tr '\n' ' '") || continue
		case $state in
		"inactive success 0 ") ;;
		"inactive success "*)
			# A clean exit also covers "not a trial"; require the bless itself.
			if ! on_tablet "journalctl -b -q -u tabs6-boot-good --no-pager | grep -q 'blessed: '"; then
				echo "deploy-over-ssh: tabs6-boot-good exited without blessing the entry; the next boot falls back" >&2
				on_tablet "journalctl -b -q -u tabs6-boot-good --no-pager | tail -5" >&2 || true
				exit 1
			fi
			select_bundle
			exit 0
			;;
		esac
	done
	echo "deploy-over-ssh: the new system did not come back blessed within 600 s; the bundle selection is unchanged" >&2
	exit 1
elif [ "$mode" = boot ] && [ -n "$bundle" ]; then
	echo "After the reboot, select the system's service bundle:"
	echo "  ssh -p $port $target '$select_cmd'"
fi

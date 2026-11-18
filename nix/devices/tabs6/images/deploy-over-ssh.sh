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
#
# It writes only the root filesystem and, through tabs6-esp, the cache ESP.
# It never opens any other partition and never flashes anything.
#
# Set by the wrapper: toplevel, firmware, placeholder, plugin_seed, sign_key,
# default_target, default_port.
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

echo "== installing ($mode)"
on_tablet "nix-env -p /nix/var/nix/profiles/system --set $toplevel && $toplevel/bin/switch-to-configuration $mode && $toplevel/sw/bin/tabs6-esp status"

if [ "$reboot" = yes ]; then
	echo "== rebooting"
	on_tablet "systemctl reboot" || true
fi

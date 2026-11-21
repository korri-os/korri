# op6-deploy: update a running OnePlus 6 over SSH (USB or Wi-Fi).
#
#   op6-deploy [--test|--switch|--boot] [--no-check-sigs] [--port N] [user@host]
#
# 1. signs the system closure with Zao's cache key and copies it (nix copy
#    over ssh-ng). --no-check-sigs is for a phone that does not trust Korri's
#    cache key yet (the first deploy from phoneputer; root is trusted there),
# 2. on a phone with no plugin receipts yet, installs the product's seeded
#    plugins (receipts, GC roots and their closures),
# 3. runs switch-to-configuration. --test (the default) activates without
#    touching the boot default, so a reboot returns to the previous system;
#    --switch and --boot also set the system profile, which Mobile NixOS
#    stage-1 boots,
# 4. selects the system's own Korri service bundle (korri-bundle-select
#    switch), as nix/devices/tabs6/images/deploy-over-ssh.sh does, unless
#    --boot.
#
# The kernel is the one in the boot.img already on the phone (default.nix),
# so this never writes a partition. Set by the wrapper: toplevel, bundle,
# plugin_seed, sign_key, default_target, default_port.
mode="test"
check_sigs=yes
target=$default_target
port=$default_port
while [ $# -gt 0 ]; do
	case $1 in
	--test) mode="test" ;;
	--switch) mode=switch ;;
	--boot) mode=boot ;;
	--no-check-sigs) check_sigs=no ;;
	--port)
		port=$2
		shift
		;;
	-h | --help)
		echo "usage: op6-deploy [--test|--switch|--boot] [--no-check-sigs] [--port N] [user@host]"
		echo "default: --test, $default_target, port $default_port"
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

ssh_opts=(-o ConnectTimeout=10 -o ServerAliveInterval=10 -p "$port")
[ -z "${OP6_KNOWN_HOSTS:-}" ] || ssh_opts+=(-o "UserKnownHostsFile=$OP6_KNOWN_HOSTS")
export NIX_SSHOPTS="${ssh_opts[*]}"
on_phone() {
	# shellcheck disable=SC2029 # expanded locally on purpose
	ssh "${ssh_opts[@]}" "$target" "$@"
}

seed=no
if on_phone "test -e /var/lib/korri-plugin-host"; then
	echo "== the phone already has plugin receipts; the plugin host keeps them"
else
	seed=yes
fi
paths=("$toplevel")
[ "$seed" = no ] || paths+=("$plugin_seed")

[ -r "$sign_key" ] || {
	echo "op6-deploy: no signing key at $sign_key (run on Zao)" >&2
	exit 1
}
echo "== signing ${paths[*]}"
nix store sign --recursive --key-file "$sign_key" "${paths[@]}"
copy_flags=()
[ "$check_sigs" = yes ] || copy_flags+=(--no-check-sigs)
echo "== copying ${paths[*]} to $target"
nix copy "${copy_flags[@]}" --to "ssh-ng://$target" "${paths[@]}"

if [ "$seed" = yes ]; then
	echo "== installing the seeded plugin receipts"
	on_phone "test ! -e /var/lib/korri-plugin-host && tar -C / -xpf $plugin_seed/seed.tar --keep-old-files"
fi

echo "== activating ($mode)"
if [ "$mode" = test ]; then
	on_phone "$toplevel/bin/switch-to-configuration test" || echo "op6-deploy: switch-to-configuration reported failed units" >&2
else
	on_phone "nix-env -p /nix/var/nix/profiles/system --set $toplevel && $toplevel/bin/switch-to-configuration $mode" || echo "op6-deploy: switch-to-configuration reported failed units" >&2
fi

if [ "$mode" = boot ]; then
	echo "After the reboot, select the system's service bundle:"
	echo "  ssh -p 2222 $target '/run/current-system/sw/bin/korri-bundle-select switch $bundle /run/current-system/sw/bin/systemctl'"
	exit 0
fi

echo "== selecting the service bundle $bundle"
on_phone "systemctl reset-failed korrid.service korrid-identity.service korri-shell.service 2>/dev/null; /run/current-system/sw/bin/korri-bundle-select switch $bundle /run/current-system/sw/bin/systemctl"
on_phone "test \"\$(readlink -f /proc/\$(systemctl show korrid -p MainPID --value)/exe)\" = \"\$(readlink -f $bundle/bin/korrid)\"" || {
	echo "op6-deploy: korrid does not run $bundle/bin/korrid" >&2
	exit 1
}
echo "korrid runs $bundle/bin/korrid"

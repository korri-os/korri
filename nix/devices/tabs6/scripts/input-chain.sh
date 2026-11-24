#!/usr/bin/env nix-shell
#!nix-shell -i bash -p systemd psmisc gnugrep gawk coreutils
# Read-only: where along evdev -> InputPlumber -> inputd -> korrid -> shell
# do the GameSir's buttons stop?
#   ssh -p 2222 root@tabs6.local bash -s < nix/devices/tabs6/scripts/input-chain.sh
export PATH=/run/wrappers/bin:/run/current-system/sw/bin:$PATH
IP=org.shadowblip.InputPlumber
echo "== GameSir evdev nodes"
grep -B1 -A5 -i "Vendor=3537" /proc/bus/input/devices | grep -E "N:|H:"
echo "== composites and their sources / targets"
for c in $(busctl --system tree $IP 2>/dev/null | grep -oE "/org/shadowblip/InputPlumber/CompositeDevice[0-9]+$"); do
	echo "$c"
	busctl --system get-property $IP "$c" org.shadowblip.Input.CompositeDevice Name 2>&1
	busctl --system get-property $IP "$c" org.shadowblip.Input.CompositeDevice SourceDevicePaths 2>&1
	busctl --system get-property $IP "$c" org.shadowblip.Input.CompositeDevice TargetDevices 2>&1
	busctl --system get-property $IP "$c" org.shadowblip.Input.CompositeDevice DbusDevices 2>&1
	busctl --system get-property $IP "$c" org.shadowblip.Input.CompositeDevice ProfileName 2>&1
done
echo "== who holds the GameSir event nodes"
while IFS= read -r n; do
	echo "$n: $(fuser -v "/dev/input/$n" 2>&1 | tail -n +2 | awk '{print $NF}' | tr '\n' ' ')"
done < <(grep -A5 -i "Vendor=3537" /proc/bus/input/devices | grep -oE "event[0-9]+")
echo "== inputd log, last 30 min (non-failure)"
journalctl -u korri-inputd --since "-30 min" -o cat --no-pager 2>/dev/null | grep -v reconcile_failed | grep -v "journal file" | tail -12 | cut -c1-260
echo "== shell log, input"
journalctl -u korri-shell --since "-30 min" -o cat --no-pager 2>/dev/null | grep -iE "input|button|press" | tail -8 | cut -c1-200

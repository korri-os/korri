#!/usr/bin/env nix-shell
#!nix-shell -i bash -p procps util-linux gnugrep gawk coreutils
# Launch Super Mario World N times through korrid; after each launch sample the
# playback PCM's hw_ptr twice, 2 s apart, then end the session.
#   ssh -p 2222 root@tabs6.local bash -s 3 < nix/devices/tabs6/scripts/smw-audio-cycle.sh
export PATH=/run/wrappers/bin:/run/current-system/sw/bin:$PATH
N=${1:-3}
[[ "$N" =~ ^[1-9][0-9]*$ ]] || {
	echo 'usage: smw-audio-cycle.sh [positive launch count]' >&2
	exit 2
}
hwptr() { grep -h "hw_ptr" /proc/asound/card0/pcm*p/sub0/status 2>/dev/null | awk '{print $3}' | tr '\n' ','; }
for i in $(seq "$N"); do
	korri play --yes "Super Mario World (USA)" || exit 1
	sleep 20
	a=$(hwptr)
	sleep 2
	b=$(hwptr)
	cpu=$(ps -o pcpu= -p "$(pgrep -f 'retroarch --config' | head -1)" 2>/dev/null | tr -d ' ')
	hang=$(dmesg | grep -c "hangcheck recover")
	if [ "$a" != "$b" ] && [ -n "$b" ]; then s=moving; else s=STUCK; fi
	echo "launch $i: audio=$s hw_ptr $a -> $b retroarch_cpu=${cpu:-none} gpu_hangs=$hang"
	korri end --yes || exit 1
	sleep 15
done

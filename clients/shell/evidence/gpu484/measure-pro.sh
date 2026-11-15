#!/usr/bin/env bash
set -euo pipefail
phase=$1
binary=$2
out=/tmp/shell-gpu-484-$phase
mkdir -p "$out"
conf=/run/systemd/system/korri-shell.service.d/484-measure.conf
[ -z "$(systemctl show korri-shell -p DropInPaths --value)" ] || { echo 'unexpected drop-in'; exit 1; }
cleanup() { rm -f "$conf"; systemctl daemon-reload; systemctl restart korri-shell; if [ -n "${input_pid:-}" ]; then kill "$input_pid"; wait "$input_pid" || :; fi; rm -f /tmp/shell-gpu-484-input.sock; if [ -n "${profiling_before:-}" ]; then echo "$profiling_before" > /sys/class/devfreq/1800000.gpu/device/profiling; fi; }
trap cleanup EXIT
if [[ "$phase" = gpu* ]]; then
 profiling_before=$(cat /sys/class/devfreq/1800000.gpu/device/profiling)
 echo 1 > /sys/class/devfreq/1800000.gpu/device/profiling
fi
mkdir -p "$(dirname "$conf")"
packages=/nix/store/pwv1zkrji6iq17v574prgm6bzmyygr52-korri-shell-surfaces
ydotool=/nix/store/7dmpnkxnagd290qpgslkxahkvw57a6wm-ydotool-1.0.4/bin/ydotool
export YDOTOOL_SOCKET=/tmp/shell-gpu-484-input.sock
"${ydotool}d" --socket-path="$YDOTOOL_SOCKET" > "$out/input.log" 2>&1 &
input_pid=$!
sleep 2
key() { "$ydotool" key "$1:1" "$1:0"; }
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=korri-wayland
for scenario in velvet-idle velvet-navigation zone-games; do
 surface=velvet
 [ "$scenario" != zone-games ] || surface=zone
 cat > "$conf" <<EOF
[Service]
ExecStart=
ExecStart=$binary --packages $packages/packages --cache $packages/cache --installed --placement fullscreen --surfaces $surface --stats
EOF
 if [[ "$phase" = gpu* ]]; then
  cat >> "$conf" <<'EOF'
BindPaths=/dev/dri/by-path/platform-1800000.gpu-render:/dev/dri/renderD128
DevicePolicy=closed
DeviceAllow=/dev/dri/by-path/platform-1800000.gpu-render rw
SupplementaryGroups=render
EOF
 fi
 systemctl daemon-reload
 systemctl restart korri-shell
 key 42
 sleep 12
 # Activity prevents attract mode while leaving Zone's GAMES page animated.
 key 42
 if [ "$scenario" = velvet-navigation ]; then key 108; sleep 2; fi
 pid=$(systemctl show korri-shell -p MainPID --value)
 start=$(date --iso-8601=seconds)
 echo "$pid $start" > "$out/$scenario.meta"
 cat /proc/$pid/maps > "$out/$scenario.maps"
 cat /proc/$pid/cgroup > "$out/$scenario.cgroup"
 ls -l /proc/$pid/fd > "$out/$scenario.fds"
 for n in $(seq 0 30); do
  [ "$(systemctl show korri-shell -p MainPID --value)" = "$pid" ] || exit 1
  { cat /proc/uptime; cat /proc/$pid/stat; cat /proc/$pid/smaps_rollup; cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq; } >> "$out/$scenario.samples"
  # Non-render fds can close between globbing and reading fdinfo.
  { cat /proc/uptime; cat /proc/$pid/fdinfo/* 2>/dev/null || :; } >> "$out/$scenario.fdinfo"
  if [ "$n" = 30 ]; then end=$(date --iso-8601=seconds); fi
  if [ "$scenario" = velvet-navigation ]; then
   key 106; sleep 0.4; key 105; sleep 0.4
  else
   if [ "$scenario" = zone-games ]; then key 103; else key 42; fi
   sleep 1
  fi
 done
 /nix/store/8svfyws68lbybhhf1466wsakqnk74bxz-grim-1.5.0/bin/grim "$out/$scenario.png"
 journalctl -u korri-shell _PID="$pid" --since "$start" --until "$end" --no-pager -o short-iso-precise > "$out/$scenario.pacing.log"
done

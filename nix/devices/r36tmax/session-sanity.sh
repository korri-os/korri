# Run only through the packaged, time-bounded r36tmax-session-sanity command.
# Read-only observations. No credentials, RPC capability or persistent output.
set -eu

printf '%s\n' '=== R36T Max session sanity (not physical display acceptance) ==='
printf 'system: '
readlink -f /run/current-system || true
printf 'uptime: '
cat /proc/uptime
printf '%s\n' '--- DRM nodes and connector state ---'
ls -l /dev/dri/by-path/ 2>&1 || true
for connector in /sys/class/drm/card*-DSI-*; do
  [ -d "$connector" ] || continue
  printf '%s\n' "$connector"
  cat "$connector/status" "$connector/enabled" "$connector/modes" 2>&1 || true
done

for sample in 1 2 3; do
  printf '\n--- memory sample %s of 3 ---\n' "$sample"
  cat /proc/uptime
  grep -E '^(MemTotal|MemAvailable|MemFree|SwapTotal|SwapFree|AnonPages|Shmem|Slab|CmaTotal|CmaFree):' /proc/meminfo
  cat /proc/pressure/memory 2>&1 || true
  grep '^oom_kill ' /proc/vmstat || true
  # Native systemd properties preserve units (MemoryCurrent/Peak are bytes).
  # Missing units on the console baseline are expected, not a passed session.
  systemctl --no-pager show \
    korri-compositor.service korrid.service korri-kiosk.service \
    static-web-server.service sunshine.service korri-inputd.service inputplumber.service \
    -p Id -p LoadState -p ActiveState -p SubState -p Result -p NRestarts \
    -p MemoryCurrent -p MemoryPeak 2>&1 || true
  # RSS is KiB, includes shared pages, and must not be summed as unique memory.
  ps -eo pid,comm,rss --sort=-rss | head -n 16
  if [ "$sample" -lt 3 ]; then sleep 5; fi
done

printf '\n%s\n' '--- bounded kernel OOM/GPU messages ---'
journalctl -k -b --no-pager -n 30 --grep='oom|Out of memory|Killed process|panfrost|drm|GPU' 2>&1 || true
printf '\n%s\n' '--- candidate service and static-origin sanity ---'
status=0
for unit in korri-compositor korrid korri-kiosk static-web-server; do
  if ! systemctl is-active "$unit.service"; then status=1; fi
done
# Static HTML only. This does not prove authenticated RPC, a rendered frame,
# interaction or a 1 GB memory fit. On the console baseline it must fail.
if ! curl --fail --silent --show-error --connect-timeout 2 --max-time 5 \
  --output /dev/null http://127.0.0.1:8099/; then status=1; fi
exit "$status"

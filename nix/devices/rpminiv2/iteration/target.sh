#!/usr/bin/env nix-shell
#!nix-shell -i bash -p bash coreutils util-linux systemd gnugrep gnused curl
# Operator test on the approved spare SD. No Nix invocation, builds, grabs or game launches.
set -euo pipefail
if [[ "${1:-}" == stage ]]; then
  [[ "$(tr -d '\0' </proc/device-tree/model)" == 'Retroid Pocket Mini V2' ]]
  [[ "$(findmnt -n -o SOURCE /)" == /dev/mmcblk0p2 ]]
  mktemp -d /run/korri-input-trial.XXXXXXXX
  exit 0
fi
if [[ "${1:-}" == verify ]]; then
  stage=${2:?staging directory}
  [[ "$stage" =~ ^/run/korri-input-trial\.[a-zA-Z0-9]+$ && "$(stat -c '%u' "$stage")" == 0 ]]
  unit=korri-chromium-kiosk.service
  [[ "$(systemctl show "$unit" -p ActiveState --value)" == active ]]
  actual_dropins=$(systemctl show "$unit" -p DropInPaths --value)
  [[ "$actual_dropins" == "$(cat "$stage/original-dropins")" ]]
  pid=$(systemctl show "$unit" -p MainPID --value)
  [[ "$(readlink -f "/proc/$pid/exe")" == "$(cat "$stage/original-executable")" ]]
  echo 'ROLLBACK VERIFIED: production executable and baseline drop-ins are active'
  exit 0
fi
stage=${1:?staging directory}
mode=${2:?baseline or isolate-portal}
expected=${3:?expected current-system path}
trace=${4:-no-trace}
action=${5:-run}
[[ "$action" == run || "$action" == check-rollback ]]
[[ "$trace" == trace || "$trace" == no-trace ]]
[[ "$stage" =~ ^/run/korri-input-trial\.[a-zA-Z0-9]+$ ]]
[[ "$mode" == baseline || "$mode" == isolate-portal ]]
[[ "$(tr -d '\0' </proc/device-tree/model)" == 'Retroid Pocket Mini V2' ]]
[[ "$(findmnt -n -o SOURCE /)" == /dev/mmcblk0p2 ]]
[[ "$(findmnt -n -o SOURCE /boot)" == /dev/mmcblk0p1 ]]
[[ "$(readlink -f /run/current-system)" == "$expected" ]]
[[ "$(findmnt -n -o FSTYPE /run)" == tmpfs ]]
[[ "$(stat -c '%u' "$stage")" == 0 ]]
readonly unit=korri-chromium-kiosk.service
readonly override=/run/systemd/system/korri-chromium-kiosk.service.d/90-input-trial.conf
[[ ! -e "$override" && ! -L "$override" ]]
baseline_dropins=$(systemctl show "$unit" -p DropInPaths --value)
[[ -z "$baseline_dropins" || "$baseline_dropins" == /run/systemd/system/korri-chromium-kiosk.service.d/60-input-view.conf ]]
printf '%s\n' "$baseline_dropins" >"$stage/original-dropins"
baseline_digest=''
if [[ -n "$baseline_dropins" ]]; then
  [[ -f "$baseline_dropins" && ! -L "$baseline_dropins" && "$(stat -c '%u' "$baseline_dropins")" == 0 ]]
  grep -Fx '# Managed by rpminiv2-input-iterate' "$baseline_dropins" >/dev/null
  baseline_digest=$(sha256sum "$baseline_dropins" | cut -d' ' -f1)
fi
[[ "$(systemctl show "$unit" -p ActiveState --value)" == active ]]
[[ "$(systemctl show "$unit" -p FreezerState --value)" == running ]]
[[ "$(systemctl show korri-inputd.service -p StatusText --value)" == Ready ]]
original_pid=$(systemctl show "$unit" -p MainPID --value)
original_exe=$(readlink -f "/proc/$original_pid/exe")
printf '%s\n' "$original_exe" >"$stage/original-executable"
original=$(systemctl show "$unit" -p ExecStart --value | grep -oE '/nix/store/[a-z0-9]{32}-korri-chromium-kiosk/bin/korri-chromium-kiosk' | head -1)
[[ -f "$original" && "$(stat -c '%u' "$original")" == 0 ]]
jq=$(grep -oE '/nix/store/[a-z0-9]{32}-jq[^ /]*/bin/jq' "$original")
[[ -x "$jq" ]]
no_game() {
  local status
  status=$(runuser -u korri-inputd -g korri-control -- curl --silent --show-error --max-time 4 --unix-socket /run/korrid-control/control.sock -H 'Content-Type: application/json' --data '{"_tag":"app.session.status","payload":{}}' http://localhost/rpc) || return 1
  printf '%s' "$status" | "$jq" -e -f "$stage/no-game.jq" >/dev/null
}
no_game
(cd "$stage" && sha256sum --check payload.sha256)
while IFS= read -r dependency; do
  [[ "$dependency" == /nix/store/* && -e "$dependency" ]]
done <"$stage/references"
chmod 0755 "$stage" "$stage/korri-portal-shell" "$stage/korri-replay-dpad"
portal=()
for phys in /sys/class/input/event*/device/phys; do
  [[ -r "$phys" ]] || continue
  if [[ "$(cat "$phys")" == korri/inputd/portal ]]; then
    name=${phys#/sys/class/input/}
    portal+=("/dev/input/${name%%/*}")
  fi
done
[[ "${#portal[@]}" == 1 ]]
portal_parent=$(readlink -f "/sys/class/input/${portal[0]##*/}/device")
printf '%s\n' "$portal_parent" >"$stage/portal-parent"
[[ "$portal_parent" =~ ^/sys/devices/virtual/input/input[0-9]+$ ]]
sed -E "s|/nix/store/[a-z0-9]{32}-korri-portal-shell-0.0.0/bin/korri-portal-shell|$stage/korri-portal-shell|" "$original" >"$stage/kiosk"
grep -F "$stage/korri-portal-shell" "$stage/kiosk" >/dev/null
chmod 0755 "$stage/kiosk"
printf '[Service]\nExecStart=\nExecStart=%s/kiosk\n' "$stage" >"$stage/override"
if [[ "$trace" == trace ]]; then
  chmod 0644 "$stage/input-trace.so"
  # Do not preload into the privileged ExecStartPre. It would create a
  # root-owned trace file that the actual browser cannot append to.
  sed -i "/^exec /i export LD_PRELOAD=$stage/input-trace.so" "$stage/kiosk"
fi
if [[ "$mode" == isolate-portal ]]; then
  printf 'TemporaryFileSystem=\n' >>"$stage/override"
  for node in /sys/class/input/js*; do
    parent=$(readlink -f "$node/device")
    [[ "$parent" =~ ^/sys/devices/virtual/input/input[0-9]+$ ]] || continue
    [[ "$parent" != "$portal_parent" ]] || continue
    # Readable empty mounts hide these subtrees without making enumeration
    # fail with EACCES while traversing the surrounding input subsystem.
    printf 'TemporaryFileSystem=%s:ro,mode=755\n' "$parent" >>"$stage/override"
  done
fi
chmod 0644 "$stage/override"
override_digest=$(sha256sum "$stage/override" | cut -d' ' -f1)
installed=0
restore() {
  trap '' HUP INT TERM
  if [[ "$installed" == 1 ]]; then
    if [[ -n "$baseline_dropins" && "$(sha256sum "$baseline_dropins" | cut -d' ' -f1)" != "$baseline_digest" ]]; then return 1; fi
    if [[ -e "$override" || -L "$override" ]]; then
      if [[ -L "$override" || ! "$override" -ef "$stage/override" ]] || [[ "$(sha256sum "$override" | cut -d' ' -f1)" != "$override_digest" ]] || ! no_game; then
        printf 'ROLLBACK FAILED: unit changed or a game became active\n' >"$stage/restore.log"
        return 1
      fi
      rm "$override" || return 1
      systemctl daemon-reload || return 1
      if ! systemctl restart "$unit"; then
        printf 'ROLLBACK FAILED: original kiosk did not start\n' >"$stage/restore.log"
        return 1
      fi
    fi
    local actual_dropins
    actual_dropins=$(systemctl show "$unit" -p DropInPaths --value) || return 1
    [[ "$actual_dropins" == "$baseline_dropins" ]] || return 1
    [[ "$(systemctl show "$unit" -p ActiveState --value)" == active ]] || return 1
    [[ "$(systemctl show "$unit" -p ExecStart --value)" == *"path=$original ;"* ]] || return 1
    local restored_pid
    restored_pid=$(systemctl show "$unit" -p MainPID --value) || return 1
    [[ "$(readlink -f "/proc/$restored_pid/exe")" == "$original_exe" ]] || return 1
    installed=0
  fi
  printf 'RESTORED: original kiosk and baseline drop-ins active; no trial drop-in\n' >"$stage/restore.log"
}
finish() {
  local code=$?
  trap - EXIT
  if ! restore; then exit 90; fi
  exit "$code"
}
trap finish EXIT
trap 'exit 130' HUP INT TERM
install -d -m 0755 "${override%/*}"
# Record ownership before the atomic link, so an interrupt cannot orphan it.
installed=1
ln "$stage/override" "$override"
if [[ "$action" == check-rollback ]]; then kill -TERM $$; fi
systemctl daemon-reload
systemctl restart "$unit"
# Portal-target replay does not traverse inputd's physical-activity notifier.
# Wake through the existing native idle notification, never a pointer event.
sway=/nix/store/vm7v8ak4ix5bni24l3cswz2iwqi9ya8k-sway-unwrapped-1.11/bin/swaymsg
runuser -u korri -- "$sway" -s /run/korri-compositor/sway-ipc.sock 'seat * idle_notify' >/dev/null
for _wake_attempt in {1..20}; do
  "$sway" -s /run/korri-compositor/sway-ipc.sock -t get_outputs -r >"$stage/outputs.json"
  "$jq" -e 'any(.[]; .name == "DSI-1" and .power == true)' "$stage/outputs.json" >/dev/null && break
  sleep 0.1
done
"$jq" -e 'any(.[]; .name == "DSI-1" and .power == true)' "$stage/outputs.json" >/dev/null
invocation=$(systemctl show "$unit" -p InvocationID --value)
[[ "$invocation" =~ ^[a-f0-9]{32}$ ]]
# All captured fields are selected and sanitized by the test-only launcher.
snapshot() {
  { journalctl -b "_SYSTEMD_INVOCATION_ID=$invocation" --grep='queried_gamepads=true state=' -n 1 -o json --no-pager --quiet || [[ "$?" == 1 ]]; } |
    "$jq" -r '.MESSAGE | split("state=")[1] | fromjson'
}
for _attempt in {1..40}; do
  snapshot >"$stage/before.json"
  [[ -s "$stage/before.json" ]] && break
  sleep 0.25
done
[[ -s "$stage/before.json" ]]
if [[ "$trace" == trace ]]; then
  main_pid=$(systemctl show "$unit" -p MainPID --value)
  chmod 0644 "$stage/inspect.sh"
  nsenter --target "$main_pid" --mount -- runuser -u korri-portal -g korri-portal -- bash "$stage/inspect.sh" >"$stage/input-view.log"
  {
    while IFS= read -r pid; do
      for comm in /proc/"$pid"/task/*/comm; do
        [[ -r "$comm" ]] || continue
        [[ "$(cat "$comm")" == 'Gamepad polling' ]] || continue
        printf 'Gamepad thread %s executable=%s\n' "$comm" "$(readlink -f "/proc/$pid/exe")"
        printf 'Mount namespaces: launcher=%s browser=%s\n' "$(readlink "/proc/$main_pid/ns/mnt")" "$(readlink "/proc/$pid/ns/mnt")"
        nsenter --target "$pid" --mount -- runuser -u korri-portal -g korri-portal -- bash "$stage/inspect.sh" >"$stage/browser-input-view.log"
        if grep -qF input-trace.so "/proc/$pid/maps"; then echo 'Trace library mapped'; else echo 'Trace library NOT mapped'; fi
        while IFS= read -r -d '' entry; do
          case "$entry" in LD_PRELOAD=* | XDG_RUNTIME_DIR=*) printf '%s\n' "$entry" ;; esac
        done <"/proc/$pid/environ"
        for fd in "${comm%/comm}"/fd/*; do
          link=$(readlink "$fd" 2>/dev/null || true)
          case "$link" in /dev/input/*) printf '%s %s\n' "$fd" "$link" ;; esac
        done
      done
    done </sys/fs/cgroup/system.slice/korri-chromium-kiosk.service/cgroup.procs
  } >"$stage/input-fds.log"
fi
no_game
joydev=()
for node in /sys/class/input/js*; do
  [[ "$(readlink -f "$node/device")" == "$portal_parent" ]] && joydev+=("/dev/input/${node##*/}")
done
[[ "${#joydev[@]}" == 1 ]]
snapshot >"$stage/before.json"
[[ -s "$stage/before.json" ]]
"$stage/korri-replay-dpad" "${portal[0]}" "${joydev[0]}" >"$stage/replay.log" 2>&1
sleep 0.5
snapshot >"$stage/after.json"
for _attempt in {1..60}; do
  [[ -f /run/korri-portal/rpmini-input-diagnostic.jpg.b64 ]] && break
  sleep 0.25
done
install -m 0600 /run/korri-portal/rpmini-input-diagnostic.jpg.b64 "$stage/screen.b64"
if [[ "$trace" == trace && -f /run/korri-portal/rpmini-input-open.log ]]; then
  install -m 0600 /run/korri-portal/rpmini-input-open.log "$stage/input-open.log"
fi
journalctl -b "_SYSTEMD_INVOCATION_ID=$invocation" --grep=DEBUG-rpmini-input -o json --no-pager |
  "$jq" -c '{__MONOTONIC_TIMESTAMP,MESSAGE}' >"$stage/probe.jsonl"
restore
cat "$stage/restore.log"
printf 'CAPTURE COMPLETE: %s\n' "$stage"

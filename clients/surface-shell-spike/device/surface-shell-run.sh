#!/bin/sh
# THROWAWAY device runner: unpack prebuilt files, run, capture, always restore.
# Root is needed ONLY to read the existing gamepad event nodes. No ACL/group edits.
set -eu
BUNDLE=/tmp/$1
PACKAGES=$2
OUT=$3
HOLD_SECONDS=$4
case "$HOLD_SECONDS" in ''|*[!0-9]*|?????*) echo 'invalid hold seconds' >&2; exit 2 ;; esac
[ "$HOLD_SECONDS" -le 3600 ] || exit 2
case "$OUT" in /tmp/surface-shell-results-*) ;; *) echo 'invalid result path' >&2; exit 2 ;; esac
GRIM=/nix/store/8svfyws68lbybhhf1466wsakqnk74bxz-grim-1.5.0/bin/grim
SWAYMSG=/nix/store/8fx1mm7z1nv7vrj65qzp4077s2a2x8i4-sway-1.12/bin/swaymsg
export SWAYSOCK=/run/korri-compositor/sway-ipc.sock
case "$1" in surface-shell-*) ;; *) echo 'invalid bundle name' >&2; exit 2 ;; esac
[ ! -e "$OUT" ] || { echo 'result path already exists' >&2; exit 2; }
mkdir -p "$OUT"
"$SWAYMSG" -t get_tree -r >"$OUT/tree-before.json"
ps -eo pid,user,args >"$OUT/processes-before.txt"
# Stop rather than interrupt a game or an unknown foreground client.
# systemd removes an empty inactive slice from cgroupfs. Its absence means
# there are no processes in that slice; unknown Sway clients still block below.
: >"$OUT/game-pids.txt"
if [ -d /sys/fs/cgroup/korri-games.slice ]; then
  find /sys/fs/cgroup/korri-games.slice -name cgroup.procs -exec cat {} \; >"$OUT/game-pids.txt"
else
  echo 'korri-games.slice absent (no Korri game processes)' >>"$OUT/notes.txt"
fi
if [ -s "$OUT/game-pids.txt" ] || grep '"app_id":' "$OUT/tree-before.json" | grep -v 'chrome-' >"$OUT/unknown-clients.txt"; then
  echo 'BLOCKED: game or unknown client exists; kiosk not stopped' | tee "$OUT/notes.txt"
  exit 3
fi
systemctl is-active -q korri-chromium-kiosk.service || { echo 'kiosk not active; refusing' >&2; exit 3; }
! systemctl is-active -q surface-shell-spike.service || { echo 'shell already active' >&2; exit 3; }
shot() {
  XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 "$GRIM" "$OUT/$1.png"
}
kiosk_stopped=0
input_stopped=0
CONTROL=/run/surface-shell-control
INVOCATION=''
# THROWAWAY (owner decision 2026-10-31): InputPlumber holds the H700 gamepad
# exclusively, so it is stopped at runtime for the test and korri-inputd
# (Requires=inputplumber.service) stops with it. restore() starts both again.
restore_input() {
  [ "$input_stopped" = 1 ] || return 0
  systemctl start inputplumber.service || echo 'inputplumber start failed' >>"$OUT/notes.txt"
  systemctl start korri-inputd.service || echo 'korri-inputd start failed' >>"$OUT/notes.txt"
  sleep 3
  systemctl is-active inputplumber.service korri-inputd.service >"$OUT/input-after.txt" 2>&1 || echo 'input stack not active after restore' >>"$OUT/notes.txt"
  input_stopped=0
}
journal() {
  if [ -n "$INVOCATION" ]; then
    journalctl "_SYSTEMD_INVOCATION_ID=$INVOCATION" --no-pager -o short-monotonic
  else
    journalctl -u surface-shell-spike.service --no-pager -o short-monotonic
  fi
}
restore() {
  trap - EXIT HUP INT TERM
  [ "$kiosk_stopped" = 1 ] || { restore_input; return 0; }
  journal >"$OUT/shell-journal.txt" 2>&1 || :
  systemctl stop surface-shell-spike.service 2>/dev/null || :
  rm -f "$CONTROL"
  restore_input
  systemctl start korri-chromium-kiosk.service || :
  sleep 20
  if ! systemctl is-active -q korri-chromium-kiosk.service || [ "$(systemctl show -p NRestarts --value korri-chromium-kiosk.service)" != 0 ]; then
    echo 'kiosk unstable; restarting unchanged compositor' >>"$OUT/notes.txt"
    systemctl stop korri-chromium-kiosk.service || :
    systemctl restart korri-compositor.service || echo 'compositor restart failed' >>"$OUT/notes.txt"
    sleep 3
    systemctl start korri-chromium-kiosk.service || echo 'kiosk start failed' >>"$OUT/notes.txt"
    sleep 20
  fi
  systemctl show -p ActiveState -p SubState -p NRestarts -p MainPID korri-chromium-kiosk.service >"$OUT/kiosk-after.txt"
  "$SWAYMSG" output '*' power on >/dev/null || :
  shot kiosk-after || :
  stable() {
    pid=$(systemctl show -p MainPID --value korri-chromium-kiosk.service)
    chrome=$(ps -C chromium -o pid=,args= | awk '/--app=data:/ {print $1}')
    [ -n "$chrome" ] || return 1
    for n in 0 1 2 3 4 5 6; do
      date -u +%FT%TZ >>"$OUT/kiosk-stability.txt"
      systemctl show -p ActiveState -p SubState -p NRestarts -p MainPID korri-chromium-kiosk.service >>"$OUT/kiosk-stability.txt"
      ps -C chromium -o pid=,args= | awk '/--app=data:/ {print; found=1} END {exit !found}' >>"$OUT/kiosk-stability.txt" || return 1
      if ! systemctl is-active -q korri-chromium-kiosk.service ||
         [ "$(systemctl show -p NRestarts --value korri-chromium-kiosk.service)" != 0 ] ||
         [ "$pid" != "$(systemctl show -p MainPID --value korri-chromium-kiosk.service)" ] ||
         [ "$chrome" != "$(ps -C chromium -o pid=,args= | awk '/--app=data:/ {print $1}')" ]; then return 1; fi
      [ "$n" = 6 ] || sleep 10
    done
  }
  if ! stable; then
    echo 'kiosk failed 60-second check; recovering unchanged compositor' >>"$OUT/notes.txt"
    systemctl stop korri-chromium-kiosk.service || :
    systemctl restart korri-compositor.service || :
    sleep 3
    systemctl start korri-chromium-kiosk.service || :
    sleep 20
    stable || { echo 'RECOVERY FAILED' >>"$OUT/notes.txt"; exit 1; }
  fi
  shot kiosk-after || :
}
trap restore EXIT
trap 'exit 1' HUP INT TERM PIPE
"$SWAYMSG" output '*' power on >/dev/null
shot kiosk-before
# A failed prior run can leave this exact extraction path. No shell service
# or unknown window is active (checked above), and earlier evidence is local.
# Keep the freshly copied archive/runner and this run's output until collection.
rm -rf -- /tmp/surface-shell-sp "$BUNDLE"
for previous in /tmp/surface-shell-results*; do
  [ "$previous" = "$OUT" ] || rm -rf -- "$previous"
done
tar xf "$BUNDLE.tar.gz" -C /tmp
[ -x "$BUNDLE/tools/wtype/run.sh" ] || { echo 'bundle must include prebuilt wtype for automated drive' >&2; exit 2; }
"$BUNDLE/lib/ld-linux-aarch64.so.1" --inhibit-cache --library-path "$BUNDLE/lib" --list "$BUNDLE/bin/surface-shell-spike" >"$OUT/loader.txt"
if grep -q 'not found' "$OUT/loader.txt"; then exit 1; fi
kiosk_stopped=1
systemctl stop korri-chromium-kiosk.service
input_stopped=1
systemctl is-active inputplumber.service korri-inputd.service >"$OUT/input-before.txt" 2>&1 || :
systemctl stop korri-inputd.service inputplumber.service
rm -f "$CONTROL"
mkfifo -m 0600 "$CONTROL"
sleep 2
systemd-run --unit=surface-shell-spike --collect \
  -p User=root -p Group=root \
  -p RuntimeDirectory=surface-shell-spike -p RuntimeDirectoryMode=0700 \
  -p BindReadOnlyPaths=/run/user/1000/korri-wayland:/run/surface-shell-spike/korri-wayland \
  -p NoNewPrivileges=yes -p ProtectHome=yes -p ProtectSystem=strict \
  -p LimitCORE=0 -p UMask=0077 \
  --setenv=XDG_RUNTIME_DIR=/run/surface-shell-spike \
  --setenv=WAYLAND_DISPLAY=korri-wayland \
  --setenv=SLINT_BACKEND=winit-software \
  --setenv=SHELL_FULLSCREEN=1 --setenv=SHELL_STATS=1 \
  --setenv=SHELL_SURFACES="$PACKAGES" \
  --setenv=SHELL_CONTROL="$CONTROL" \
  -p ReadWritePaths="$CONTROL" \
  "$BUNDLE/run.sh"
sleep 5
systemctl is-active -q surface-shell-spike.service
INVOCATION=$(systemctl show -p InvocationID --value surface-shell-spike.service)
printf '%s\n' "$INVOCATION" >"$OUT/invocation-id.txt"
shot surface-start
# Scripted presses go through the shell's control FIFO; Wayland key injection
# (wtype) did not reach the window under korri-compositor on 2026-10-31.
exec 9>"$CONTROL"
key() {
  case "$1" in Right) a=right ;; Left) a=left ;; F2) a=switch ;; *) a=$1 ;; esac
  printf '%s\n' "$a" >&9
}
key Right
sleep 1
shot surface-right
key Left
sleep 1
shot surface-left
CG=$(systemctl show -p ControlGroup --value surface-shell-spike.service)
printf '%s\n' "$PACKAGES" >"$OUT/packages.txt"
shot phase-0
sh /tmp/surface-shell-measure.sh "$CG" "$OUT/phase-0.csv" 24 >"$OUT/phase-0.txt"
key F2
sleep 5
shot phase-1
sh /tmp/surface-shell-measure.sh "$CG" "$OUT/phase-1.csv" 24 >"$OUT/phase-1.txt"
key F2
sleep 3
shot back-to-first
printf 'Interactive observation: %s seconds. L1+R1 switches surfaces. InputPlumber is stopped; the shell reads the H700 Gamepad directly.\n' "$HOLD_SECONDS"
sleep "$HOLD_SECONDS"
"$SWAYMSG" -t get_tree -r >"$OUT/tree-shell.json"
systemctl show -p User -p Group -p MainPID -p ActiveState surface-shell-spike.service >"$OUT/shell-unit.txt"
journal >"$OUT/shell-journal.txt"
grep 'SHELL_STATS.*package=' "$OUT/shell-journal.txt" >"$OUT/switch-timings.txt" || :
echo 'done; root used for gamepad read only, no device permission changes' >>"$OUT/notes.txt"

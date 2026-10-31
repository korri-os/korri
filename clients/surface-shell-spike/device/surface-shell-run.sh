#!/bin/sh
# THROWAWAY device runner: unpack prebuilt files, run, capture, always restore.
# Root is needed ONLY to read the existing gamepad event nodes. No ACL/group edits.
set -eu
BUNDLE=/tmp/$1
PACKAGES=$2
OUT=$3
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
restore() {
  trap - EXIT HUP INT TERM
  [ "$kiosk_stopped" = 1 ] || return 0
  journalctl -u surface-shell-spike.service --no-pager -o short-monotonic >"$OUT/shell-journal.txt" 2>&1 || :
  systemctl stop surface-shell-spike.service 2>/dev/null || :
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
  pid=$(systemctl show -p MainPID --value korri-chromium-kiosk.service)
  sleep 10
  systemctl show -p ActiveState -p SubState -p NRestarts -p MainPID korri-chromium-kiosk.service >"$OUT/kiosk-stability.txt"
  if ! systemctl is-active -q korri-chromium-kiosk.service ||
     [ "$(systemctl show -p NRestarts --value korri-chromium-kiosk.service)" != 0 ] ||
     [ "$pid" != "$(systemctl show -p MainPID --value korri-chromium-kiosk.service)" ]; then
    echo 'RECOVERY FAILED: kiosk is not stable' >>"$OUT/notes.txt"
    exit 1
  fi
}
trap restore EXIT
trap 'exit 1' HUP INT TERM
"$SWAYMSG" output '*' power on >/dev/null
shot kiosk-before
[ ! -e "$BUNDLE" ] || { echo 'bundle already extracted; choose a fresh name' >&2; exit 2; }
tar xf "$BUNDLE.tar.gz" -C /tmp
[ -x "$BUNDLE/tools/wtype/run.sh" ] || { echo 'bundle must include prebuilt wtype for automated drive' >&2; exit 2; }
"$BUNDLE/lib/ld-linux-aarch64.so.1" --inhibit-cache --library-path "$BUNDLE/lib" --list "$BUNDLE/bin/surface-shell-spike" >"$OUT/loader.txt"
if grep -q 'not found' "$OUT/loader.txt"; then exit 1; fi
kiosk_stopped=1
systemctl stop korri-chromium-kiosk.service
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
  "$BUNDLE/run.sh"
sleep 5
systemctl is-active -q surface-shell-spike.service
shot surface-start
key() {
  XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 "$BUNDLE/tools/wtype/run.sh" -k "$1"
}
key Right
sleep 1
shot surface-right
key Left
sleep 1
shot surface-left
for i in 1 2 3; do
  key F2
  sleep 3
  shot "switch-$i"
done
sleep 5
"$SWAYMSG" -t get_tree -r >"$OUT/tree-shell.json"
systemctl show -p User -p Group -p MainPID -p ActiveState surface-shell-spike.service >"$OUT/shell-unit.txt"
echo 'done; root used for gamepad read only, no device permission changes' >>"$OUT/notes.txt"

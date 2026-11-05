#!/bin/sh
# Device side, root. Acceptance run of the native shell over the fixture korrid:
# a scripted pad session on Zone and Pico with a screenshot, RSS and CPU per step.
#   sh /tmp/korri-accept-run.sh PAD_EVDEV HOLD_SECONDS
# /tmp only; nothing is built or installed; the kiosk is restored by a trap.
set -u
PAD=$1
HOLD=${2:-0}
B=/tmp/korri-preview
R=/run/korri-preview
OUT=/tmp/korri-accept-results
SWAYSOCK=/run/korri-compositor/sway-ipc.sock
SWAYMSG=$(ls -d /nix/store/*-sway-unwrapped-*/bin/swaymsg 2>/dev/null | head -1)
GRIM=$(ls -d /nix/store/*-grim-*/bin/grim 2>/dev/null | head -1)
rm -rf "$OUT"; mkdir -p "$OUT"; chmod 0777 "$OUT"
note() { echo "$(date +%T) $*" | tee -a "$OUT/notes.txt"; }

if [ -n "$(find /sys/fs/cgroup/korri.slice/korri-games.slice -name cgroup.procs -exec cat {} \; 2>/dev/null)" ]; then
  note "BLOCKED: a game is running"; exit 3
fi
systemctl is-active -q korri-chromium-kiosk.service || { note "kiosk not active; refusing"; exit 3; }

restore() {
  trap - EXIT HUP INT TERM
  exec 3>&- 2>/dev/null || :
  journalctl -u korri-accept-shell.service --no-pager -o short-precise >"$OUT/shell.log" 2>&1 || :
  journalctl -u korri-accept-korrid.service --no-pager -o short-precise >"$OUT/korrid.log" 2>&1 || :
  cp "$R/fixture.err" "$OUT/fixture.err" 2>/dev/null || :
  cp "$R/fixture.out" "$OUT/fixture.out" 2>/dev/null || :
  systemctl stop korri-accept-shell.service korri-accept-korrid.service 2>/dev/null || :
  systemctl start inputplumber.service || note "inputplumber start failed"
  systemctl start korri-inputd.service || note "inputd start failed"
  systemctl start korri-chromium-kiosk.service || :
  sleep 20
  if ! systemctl is-active -q korri-chromium-kiosk.service; then
    note "kiosk not active after restore; restarting the compositor unchanged"
    systemctl restart korri-compositor.service || :
    sleep 3
    systemctl start korri-chromium-kiosk.service || :
    sleep 20
  fi
  systemctl is-active korri-chromium-kiosk.service inputplumber.service korri-inputd.service >"$OUT/after.txt" 2>&1 || :
  rm -rf "$R" "$B"
  note "restored"
}
trap restore EXIT
trap 'exit 1' HUP INT TERM

rm -rf "$B" "$R"; tar xf /tmp/korri-preview.tar.gz -C /tmp || exit 1
mkdir -p "$R/cred" "$R/cache"
mkfifo "$R/pad.fifo"

# The display: DPMS on, and the backlight's state before and after.
for b in /sys/class/backlight/*; do note "backlight before: $b $(cat $b/brightness)/$(cat $b/max_brightness) bl_power=$(cat $b/bl_power)"; done
SWAYSOCK=$SWAYSOCK "$SWAYMSG" "output * power on" >>"$OUT/notes.txt" 2>&1 || note "swaymsg power on failed"
SWAYSOCK=$SWAYSOCK "$SWAYMSG" "output * dpms on" >>"$OUT/notes.txt" 2>&1 || :
sleep 1
for b in /sys/class/backlight/*; do note "backlight after: $b $(cat $b/brightness)/$(cat $b/max_brightness) bl_power=$(cat $b/bl_power)"; done

systemctl stop korri-chromium-kiosk.service
systemctl stop korri-inputd.service inputplumber.service
sleep 2

# The fixture korrid reads the real pad (every button logged as PAD) and the
# scripted presses from the fifo.
systemd-run --unit=korri-accept-korrid --collect \
  sh -c "exec $B/run-fixture.sh --games 40 --credentials $R/cred --evdev $PAD <$R/pad.fifo >$R/fixture.out 2>$R/fixture.err"
exec 3>"$R/pad.fifo"
for i in $(seq 100); do grep -q KORRID_PORTAL_ORIGIN "$R/fixture.out" 2>/dev/null && break; sleep 0.1; done
ADDR=$(sed -n 's/^KORRID_ADDRESS=//p' "$R/fixture.out")
ORIGIN=$(sed -n 's/^KORRID_PORTAL_ORIGIN=//p' "$R/fixture.out")
[ -n "$ADDR" ] || { note "fixture korrid did not start"; cat "$R/fixture.err" >>"$OUT/notes.txt"; exit 1; }
chown -R korri-portal:korri-portal "$R"; chmod 0700 "$R/cred"

systemd-run --unit=korri-accept-shell --collect \
  -p User=korri-portal -p Group=korri-portal \
  -p RuntimeDirectory=korri-accept-shell -p RuntimeDirectoryMode=0700 \
  -p BindReadOnlyPaths=/run/user/1000/korri-wayland:/run/korri-accept-shell/korri-wayland \
  -p NoNewPrivileges=yes -p ProtectHome=yes \
  --setenv=XDG_RUNTIME_DIR=/run/korri-accept-shell --setenv=WAYLAND_DISPLAY=korri-wayland \
  --setenv=KORRID_ADDRESS="$ADDR" --setenv=KORRID_PORTAL_ORIGIN="$ORIGIN" \
  --setenv=CREDENTIALS_DIRECTORY="$R/cred" \
  sh -c "$B/run-shell.sh install --packages $B/packages --cache $R/cache && exec $B/run-shell.sh --packages $B/packages --cache $R/cache --surfaces zone,pico --placement fullscreen --stats"

shell_pid() { systemctl show -p MainPID --value korri-accept-shell.service; }
STEP=0
shot() { # shot NAME: screenshot, RSS and CPU of the shell
  STEP=$((STEP + 1))
  name=$(printf '%02d-%s' "$STEP" "$1")
  sleep "${2:-1.5}"
  pid=$(pgrep -f "bin/korri-shell --packages" | head -1)
  rss=$(awk '/VmRSS/{print $2}' /proc/$pid/status 2>/dev/null)
  ticks=$(awk '{print $14+$15}' /proc/$pid/stat 2>/dev/null)
  runuser -u korri -- env XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 "$GRIM" "$OUT/$name.png" 2>>"$OUT/notes.txt" || note "grim failed for $name"
  alive=$(systemctl is-active korri-accept-shell.service)
  echo "STEP $name shell=$alive pid=$pid rss_kib=$rss cpu_ticks=$ticks" | tee -a "$OUT/steps.txt"
}
press() { for k in "$@"; do echo "press $k" >&3; sleep 0.35; done; }
switch() { echo "hold 0x136" >&3; sleep 0.05; echo "hold 0x137" >&3; sleep 0.3; echo "release 0x137" >&3; echo "release 0x136" >&3; }
UP=0x220 DOWN=0x221 LEFT=0x222 RIGHT=0x223 A=0x130 B=0x131 MENU=0x13b

for i in $(seq 1800); do pgrep -f "bin/korri-shell --packages" >/dev/null && break; systemctl is-active -q korri-accept-shell.service || break; sleep 0.1; done
note "shell process: $(pgrep -f "bin/korri-shell --packages" | head -1) after install"
sleep 8
for surface in zone pico; do
  shot "$surface-start" 1
  press $RIGHT; shot "$surface-right"
  press $DOWN; shot "$surface-down"
  press $LEFT; shot "$surface-left"
  press $UP; shot "$surface-up"
  press $DOWN $A; shot "$surface-open-game" 2
  press $MENU; shot "$surface-menu"
  press $B; shot "$surface-menu-closed"
  press $A; shot "$surface-launch" 3
  echo "requests" >&3
  echo "overlay on" >&3; shot "$surface-overlay" 3
  press $B; shot "$surface-overlay-back" 2
  echo "overlay off" >&3; shot "$surface-overlay-off" 2
  press $B $B $B; shot "$surface-back-out" 2
  shot "$surface-attract" 50
  switch; shot "$surface-switched" 4
done
shot "zone-again" 2
echo "requests" >&3
if [ "$HOLD" -gt 0 ]; then
  note "Shell left running for $HOLD s for hands-on use (pad forwarded; L1+R1 switches)."
  sleep "$HOLD"
  shot "after-hands-on" 1
fi
note "script done"

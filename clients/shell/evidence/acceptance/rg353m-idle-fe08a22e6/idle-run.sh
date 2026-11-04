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
OUT=/tmp/korri-idle-results
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
  [ -f "$OUT/saved-max" ] && while read -r p f; do echo "$f" > "$p/scaling_max_freq"; done < "$OUT/saved-max"
  systemctl start korri-limits.socket korri-limits.service || note "limits start failed"
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
# 1.8 GHz for the measurement: the limits daemon would put the deployed 61 % cap back.
systemctl stop korri-limits.service korri-limits.socket
: > "$OUT/saved-max"
for p in /sys/devices/system/cpu/cpufreq/policy*; do echo "$p $(cat $p/scaling_max_freq)" >> "$OUT/saved-max"; cat $p/cpuinfo_max_freq > $p/scaling_max_freq; done
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

shellpid() { pgrep -f "bin/korri-shell --packages" | head -1; }
fixpid() { pgrep -f "bin/fixture-korrid" | head -1; }
measure() { # measure NAME SECONDS
  p=$(shellpid); f=$(fixpid)
  t0=$(awk '{print $14+$15}' /proc/$p/stat); ft0=$(awk '{print $14+$15}' /proc/$f/stat)
  tis0=$(tr '\n' ' ' < /sys/devices/system/cpu/cpufreq/policy0/stats/time_in_state)
  sleep "$2"
  t1=$(awk '{print $14+$15}' /proc/$p/stat); ft1=$(awk '{print $14+$15}' /proc/$f/stat)
  tis1=$(tr '\n' ' ' < /sys/devices/system/cpu/cpufreq/policy0/stats/time_in_state)
  tis=$(echo "$tis0|$tis1" | awk -F'|' '{n=split($1,a," "); split($2,b," "); s=""; for(i=1;i<=n;i+=2){d=b[i+1]-a[i+1]; if(d>0) s=s a[i]/1000 ":" d ","} print s}')
  echo "MEASURE $1 secs=$2 shell_cpu_pct=$(( (t1-t0) * 100 / ($2 * 100) )) fixture_cpu_pct=$(( (ft1-ft0) * 100 / ($2 * 100) )) max=$(cat /sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq) time_in_state=$tis threads=$(ls /proc/$p/task | wc -l)" | tee -a "$OUT/measure.txt"
  for q in $p $f; do echo "== $1 pid $q $(tr '\0' ' ' < /proc/$q/cmdline | cut -c1-80)" >> "$OUT/smaps.txt"; cat /proc/$q/smaps_rollup >> "$OUT/smaps.txt"; awk '/^[0-9a-f]+-[0-9a-f]+ /{name=$6} /^Rss:/{r[name]+=$2} END{for(n in r) if (r[n]>=512) printf "%8d KiB %s\n", r[n], n}' /proc/$q/smaps | sort -rn >> "$OUT/smaps.txt"; done
}
switch() { echo "hold 0x136" >&3; sleep 0.05; echo "hold 0x137" >&3; sleep 0.3; echo "release 0x137" >&3; echo "release 0x136" >&3; }
for i in $(seq 1800); do pgrep -f "bin/korri-shell --packages" >/dev/null && break; systemctl is-active -q korri-accept-shell.service || break; sleep 0.1; done
sleep 10
measure zone-idle 60
switch; sleep 8
measure pico-idle 60
switch; sleep 8
measure zone-again-idle 30
note "script done"

#!/bin/sh
# Device-side, runs on the RG35XX SP as root. Plain POSIX sh: the device has
# no build tools and must not fetch any. Runs the bundled native Zone port in
# place of the Chromium kiosk, measures it like the Chromium baseline, and
# always restores the kiosk.
set -u
OUT=/tmp/zone-native-results
BUNDLE=/tmp/zone-native-spike-sp
GRIM=/nix/store/8svfyws68lbybhhf1466wsakqnk74bxz-grim-1.5.0/bin/grim
SWAYMSG=/nix/store/8fx1mm7z1nv7vrj65qzp4077s2a2x8i4-sway-1.12/bin/swaymsg
export SWAYSOCK=/run/korri-compositor/sway-ipc.sock
rm -rf "$OUT"
mkdir -p "$OUT"

shot() {
  XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 "$GRIM" "$OUT/$1.png" || echo "grim failed: $1" >>"$OUT/notes.txt"
}

restore() {
  journalctl -u zone-spike-native.service --no-pager -o short-monotonic >"$OUT/native-journal.txt" 2>&1 || :
  systemctl stop zone-spike-native.service 2>/dev/null || :
  systemctl start korri-chromium-kiosk.service
  sleep 20
  if ! systemctl is-active -q korri-chromium-kiosk.service || [ "$(systemctl show -p NRestarts --value korri-chromium-kiosk.service)" != "0" ]; then
    echo "kiosk unstable after start; restarting compositor" >>"$OUT/notes.txt"
    systemctl stop korri-chromium-kiosk.service
    systemctl restart korri-compositor.service
    sleep 3
    systemctl start korri-chromium-kiosk.service
    sleep 20
  fi
  systemctl show -p ActiveState -p SubState -p NRestarts korri-chromium-kiosk.service >"$OUT/kiosk-after.txt"
  shot kiosk-after
}
trap restore EXIT

cd /tmp || exit 1
rm -rf "$BUNDLE"
tar xf /tmp/zone-native-spike-sp.tar.gz -C /tmp || exit 1
"$BUNDLE/lib/ld-linux-aarch64.so.1" --inhibit-cache --library-path "$BUNDLE/lib" --list "$BUNDLE/bin/zone-native-spike" >"$OUT/list.txt" 2>&1 || exit 1
if grep -q 'not found' "$OUT/list.txt"; then echo "missing library" >>"$OUT/notes.txt"; exit 1; fi

"$SWAYMSG" -t get_outputs -r | grep -E '"(power|dpms)"' >"$OUT/output-before.txt" 2>&1 || :
grep MemAvailable /proc/meminfo >"$OUT/memavailable-before.txt"

systemctl stop korri-chromium-kiosk.service
sleep 2
grep MemAvailable /proc/meminfo >"$OUT/memavailable-no-kiosk.txt"
cat /proc/uptime >"$OUT/start-uptime.txt"
systemd-run --unit=zone-spike-native --collect \
  -p User=korri-portal -p Group=korri-portal \
  -p RuntimeDirectory=zone-spike-native -p RuntimeDirectoryMode=0700 \
  -p BindReadOnlyPaths=/run/user/1000/korri-wayland:/run/zone-spike-native/korri-wayland \
  -p NoNewPrivileges=yes -p ProtectHome=yes -p ProtectSystem=strict \
  -p LimitCORE=0 -p UMask=0077 \
  --setenv=XDG_RUNTIME_DIR=/run/zone-spike-native \
  --setenv=WAYLAND_DISPLAY=korri-wayland \
  --setenv=SLINT_BACKEND=winit-software \
  --setenv=ZONE_SPIKE_FULLSCREEN=1 \
  --setenv=ZONE_SPIKE_STATS=1 \
  "$BUNDLE/run.sh" || exit 1

sleep 5
systemctl is-active -q zone-spike-native.service || { echo "native exited early" >>"$OUT/notes.txt"; exit 1; }
CG=$(systemctl show -p ControlGroup --value zone-spike-native.service)
echo "$CG" >"$OUT/cgroup.txt"
shot native-start

for i in 1 2 3; do
  sh /tmp/zone-spike-measure.sh "$CG" "$OUT/native-games-$i.csv" 24 >"$OUT/native-games-$i.txt" 2>&1
  sleep 5
done
sh /tmp/zone-spike-measure.sh "$CG" "$OUT/native-60s.csv" 60 >"$OUT/native-60s.txt" 2>&1
shot native-end-a
sleep 1
shot native-end-b
ps -o pid,user,rss,args -C zone-native-spike >"$OUT/native-ps.txt" 2>&1 || :
echo done >>"$OUT/notes.txt"

#!/usr/bin/env bash
# The shipped launcher and native SDL backend, on workstation SDL under Xvfb.
set -euo pipefail
release=$(realpath "${1:?release directory}")
out=$(realpath -m "${2:?output directory}")
crate=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$out"
work=$(mktemp -d)
pids=()
cleanup() {
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || :; done
    wait 2>/dev/null || :
    rm -rf "$work"
}
trap cleanup EXIT
mkdir "$work/app" "$work/home"
python3 - "$crate/webos" "$work/app" "$release/korri-shell" <<'PY'
import pathlib, sys
sys.path.insert(0, sys.argv[1])
from package import stage_surfaces
stage_surfaces(pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3]))
PY
mkdir "$work/app/bin"
cp "$release/korri-shell" "$release/fixture-korrid" "$work/app/bin/"
cp "$crate/webos/launch.sh" "$work/app/"
# Xvfb chooses an unused display; there is no race with another proof.
Xvfb -displayfd 3 -screen 0 1920x1080x24 -nolisten tcp 3>"$work/display" >"$out/xvfb.log" 2>&1 &
pids+=($!)
for _ in $(seq 100); do [[ -s "$work/display" ]] && break; sleep 0.1; done
DISPLAY=":$(cat "$work/display")"
export DISPLAY
unset WAYLAND_DISPLAY
# The launcher keeps its work directory and stderr log under TMPDIR, as on the TV.
mkdir -p "$work/tmp"
export TMPDIR="$work/tmp"
HOME="$work/home" "$work/app/launch.sh" >"$out/shell.log" 2>&1 &
launcher=$!
pids+=("$launcher")
id=''
for _ in $(seq 300); do
    id=$(xdotool search --onlyvisible --name '^Korri$' 2>/dev/null | head -1 || :)
    [[ -n "$id" ]] && break
    kill -0 "$launcher"
    sleep 0.1
done
[[ -n "$id" ]] || { echo 'no SDL window' >&2; exit 1; }
# Zone compilation and model delivery happen after SDL creates its window.
sleep 8
# SDL can replace its X11 window while entering fullscreen. Re-read the
# final window for input; capture the full output rather than a stale XID.
id=$(xdotool search --onlyvisible --name '^Korri$' | head -1)
xdotool getwindowgeometry "$id" >"$out/window.txt"
import -window root "$out/zone-1920x1080.png"
xdotool windowfocus --sync "$id" key F2
sleep 5
import -window root "$out/pico-1920x1080.png"
xdotool windowfocus --sync "$id" key F2
sleep 5
import -window root "$out/zone-return-1920x1080.png"
for image in zone pico zone-return; do
    test "$(identify -format '%wx%h' "$out/$image-1920x1080.png")" = 1920x1080
done
if cmp -s "$out/zone-1920x1080.png" "$out/pico-1920x1080.png"; then
    echo 'surface switch did not change the frame' >&2
    exit 1
fi
kill "$launcher"
wait "$launcher" || :
# Normal signal cleanup must remove the ephemeral credentials and fixture.
find "$work/home" -mindepth 1 -print >"$out/home-files.txt"
# Fontconfig may create its own non-secret cache in HOME. Only this app's
# launch directory and credential must disappear.
test -z "$(find "$work/home" -maxdepth 1 -name 'korri-shell.*' -print -quit)"
# The launcher also passes the shell's existing --surfaces ordering through.
HOME="$work/home" "$work/app/launch.sh" --surfaces pico,zone >"$out/pico-start.log" 2>&1 &
launcher=$!
pids+=("$launcher")
sleep 8
kill -0 "$launcher"
import -window root "$out/pico-start-1920x1080.png"
kill "$launcher"
wait "$launcher" || :
test -z "$(find "$work/home" -maxdepth 1 -name 'korri-shell.*' -print -quit)"
echo 'SDL_1920x1080_ZONE_PICO_ZONE_LAUNCHER_CLEANUP_PASS'

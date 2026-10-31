#!/usr/bin/env bash
# THROWAWAY: unchanged real shell, largest list, no device involvement.
set -euo pipefail
ulimit -c 0
bin=$(realpath "$1")
models=$(realpath "$2")
out=$(realpath -m "$3")
n=${4:-50000}
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$out"
stage=$(mktemp -d)
Xvfb :97 -screen 0 640x480x24 -nolisten tcp > "$out/xvfb.log" 2>&1 & xvfb=$!
pid=''
cleanup() { [[ -z $pid ]] || kill "$pid" 2>/dev/null || :; kill "$xvfb" 2>/dev/null || :; wait "$xvfb" 2>/dev/null || :; rm -rf "$stage"; }
trap cleanup EXIT
mkdir "$stage/fixtures"
mkfifo "$stage/control"
ln -s "$root/surfaces" "$stage/surfaces"
ln -s "$models/model-$n.json" "$stage/fixtures/model.json"
for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X97 ]] && break; sleep .1; done
export DISPLAY=:97 SHELL_ROOT=$stage SHELL_STATS=1 SLINT_BACKEND=winit-software SHELL_CONTROL=$stage/control
unset WAYLAND_DISPLAY
for surface in zone pico; do
    (ulimit -v 4194304; export SHELL_SURFACES=$surface; exec "$bin") > "$out/$surface.stdout" 2> "$out/$surface.stats" & pid=$!
    printf 'seconds,rss_kib,hwm_kib\n' > "$out/$surface.rss.csv"
    for second in $(seq 0 29); do
        if ! kill -0 "$pid" 2>/dev/null; then echo "shell exited early" >> "$out/$surface.stats"; break; fi
        awk -v s="$second" '/^VmRSS:/{rss=$2}/^VmHWM:/{hwm=$2}END{print s "," rss "," hwm}' "/proc/$pid/status" >> "$out/$surface.rss.csv"
        if [[ $second == 10 ]]; then import -window root "$out/$surface-before.png"; fi
        if [[ $second == 12 ]]; then printf 'right\n' > "$stage/control"; fi
        if [[ $second == 20 ]]; then import -window root "$out/$surface.png"; fi
        sleep 1
    done
    kill "$pid" 2>/dev/null || :; wait "$pid" || :; pid=''
done

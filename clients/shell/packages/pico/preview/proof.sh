#!/usr/bin/env bash
# Pico against the shell's fixture korrid, under Xvfb: the cursor reports
# FocusGame, Find asks MoveCatalog, the face is remembered and comes back
# after a restart, and the gameplay overlay shows and resumes.
#
#   preview/proof.sh SHELL FIXTURE_KORRID MODEL OUT_DIR
#
# Needs Xvfb, xdotool and ImageMagick on PATH and the X11 libraries on
# LD_LIBRARY_PATH (as `nix run .#shell-proof` sets them). Writes screenshots
# and requests.jsonl (every request the fixture received) to OUT_DIR.
set -euo pipefail
shell=$(realpath "$1")
fixture=$(realpath "$2")
model=$(realpath "$3")
out=$(realpath -m "$4")
packages=$(cd "$(dirname "$0")/../.." && pwd)
mkdir -p "$out"
work=$(mktemp -d /tmp/pico-proof.XXXXXX)
display=:131
pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || :; done
  wait 2>/dev/null || :
  rm -rf "$work"
}
trap cleanup EXIT

Xvfb "$display" -screen 0 640x480x24 -nolisten tcp >/dev/null 2>&1 &
pids+=($!)
for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${display#:} ]] && break; sleep 0.1; done
export DISPLAY=$display
unset WAYLAND_DISPLAY
mkdir -m 700 "$work/credentials" "$work/cache"
mkfifo "$work/control"
"$fixture" --games 0 --credentials "$work/credentials" --model "$model" \
  --displays '[{"id":"proof","main":true,"x":0,"y":0,"width":640,"height":480,"touch":false}]' \
  <"$work/control" >"$work/fixture.out" 2>"$out/fixture.log" &
pids+=($!)
exec 7>"$work/control"
for _ in $(seq 100); do grep -q KORRID_SHELL_ORIGIN "$work/fixture.out" 2>/dev/null && break; sleep 0.05; done
export KORRID_ADDRESS KORRID_SHELL_ORIGIN CREDENTIALS_DIRECTORY=$work/credentials
KORRID_ADDRESS=$(sed -n 's/^KORRID_ADDRESS=//p' "$work/fixture.out")
KORRID_SHELL_ORIGIN=$(sed -n 's/^KORRID_SHELL_ORIGIN=//p' "$work/fixture.out")
"$shell" install --packages "$packages" --cache "$work/cache" >"$out/install.log" 2>&1

start() {
  "$shell" --packages "$packages" --cache "$work/cache" --surfaces pico --placement 640x480 >>"$out/shell.log" 2>&1 &
  shell_pid=$!
  pids+=("$shell_pid")
}
window() {
  local id=''
  for _ in $(seq 200); do
    id=$(xdotool search --onlyvisible --name "^$1\$" 2>/dev/null | tail -1 || :)
    [[ -n $id ]] && { sleep 1; echo "$id"; return; }
    sleep 0.1
  done
  echo "no window $1" >&2
  return 1
}
keys() { local id=$1; shift; for k in "$@"; do xdotool windowfocus --sync "$id" key "$k"; sleep 0.35; done; }
shot() { sleep 0.6; import -window "$1" "$out/$2.png"; }
requests() { echo requests >&7; sleep 0.5; }

start
id=$(window Pico)
shot "$id" 1-home
keys "$id" Right Right
shot "$id" 2-cursor-on-tetris
keys "$id" o Right Right Right Right Right Right Right Return
shot "$id" 3-find-h
keys "$id" Escape
shot "$id" 4-home-again
# Settings › PICO › Font: MENU, SETTINGS, the last spine, its row.
keys "$id" Down Return Down Return Right Right Right Right Right Right Down Return
shot "$id" 5-font-changed
echo "stored pico" >&7
sleep 0.5
kill "$shell_pid"
wait "$shell_pid" 2>/dev/null || :
start
id=$(window Pico)
shot "$id" 6-restart-keeps-font
echo "overlay on" >&7
id=$(window "Pico overlay")
shot "$id" 7-overlay
keys "$id" Escape
id=$(window Pico)
shot "$id" 8-resumed
echo requests >&7
sleep 1
grep -E '^\{' "$work/fixture.out" >"$out/requests.jsonl" || :
grep -vE '^\{|^KORRID_' "$work/fixture.out" >"$out/stored.txt" || :
echo "proof in $out"

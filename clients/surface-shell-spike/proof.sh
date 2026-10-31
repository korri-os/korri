#!/usr/bin/env bash
# THROWAWAY: the real shell under Xvfb, with F2 and arrow input.
# Run with Xvfb, xdotool and ImageMagick on PATH. See README.md.
set -euo pipefail
bin=$(realpath "$1")
out=$(realpath -m "$2")
root=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
display=${SHELL_PROOF_DISPLAY:-:96}
Xvfb "$display" -screen 0 1280x720x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
xvfb=$!
pid=''
cleanup() { [[ -z $pid ]] || kill "$pid" 2>/dev/null || :; kill "$xvfb" 2>/dev/null || :; }
trap cleanup EXIT
for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${display#:} ]] && break; sleep .1; done
export DISPLAY=$display SHELL_ROOT=$root SHELL_STATS=1 SLINT_BACKEND=winit-software
unset WAYLAND_DISPLAY
window() {
  local id=''
  for _ in $(seq 100); do
    id=$(xdotool search --onlyvisible --name "$1" 2>/dev/null | head -1 || :)
    [[ -n $id ]] && { sleep .3; echo "$id"; return; }
    sleep .1
  done
  echo "no window $1" >&2; return 1
}
shot() { import -window "$1" "$out/$2.png"; }
key() { xdotool windowfocus --sync "$1" key "$2"; sleep .3; }
SHELL_SURFACES=_test/a,_test/b "$bin" 2>"$out/xvfb-stats.log" & pid=$!
a=$(window 'Shell test A')
shot "$a" a-start
key "$a" Right
shot "$a" a-right
key "$a" Left
shot "$a" a-left
key "$a" F2
b=$(window 'Shell test B')
shot "$b" b-start
if xdotool search --onlyvisible --name 'Shell test A' >/dev/null 2>&1; then echo 'old A still visible' >&2; exit 1; fi
key "$b" Right
shot "$b" b-right
key "$b" F2
a=$(window 'Shell test A')
shot "$a" a-reset
sleep 6
kill "$pid"; wait "$pid" || :; pid=''
# Pixel assertions: action changed the view, left restored it, switch reset state.
compare -metric AE "$out/a-start.png" "$out/a-left.png" null: 2>"$out/a-left-difference.txt"
compare -metric AE "$out/a-start.png" "$out/a-reset.png" null: 2>"$out/a-reset-difference.txt"
if compare -metric AE "$out/a-start.png" "$out/a-right.png" null: 2>"$out/a-right-difference.txt"; then echo 'right did not move' >&2; exit 1; fi
if compare -metric AE "$out/a-start.png" "$out/b-start.png" null: 2>"$out/f2-difference.txt"; then echo 'F2 did not switch' >&2; exit 1; fi
# A bad next package must not remove the active window or break input.
SHELL_SURFACES=_test/a,_test/missing "$bin" 2>"$out/compile-failure.log" & pid=$!
a=$(window 'Shell test A')
key "$a" F2
shot "$a" compile-failure-kept-a
key "$a" Right
shot "$a" compile-failure-still-moves
kill "$pid"; wait "$pid" || :; pid=''
grep -q 'current surface kept' "$out/compile-failure.log"
compare -metric AE "$out/a-start.png" "$out/compile-failure-kept-a.png" null: 2>"$out/compile-failure-retained-difference.txt"
if compare -metric AE "$out/compile-failure-kept-a.png" "$out/compile-failure-still-moves.png" null: 2>"$out/compile-failure-move-difference.txt"; then echo 'input failed after compile failure' >&2; exit 1; fi
# The test surface must also size entirely from its container.
SHELL_SIZE=1280x720 SHELL_SURFACES=_test/a,_test/b "$bin" 2>"$out/xvfb-720.log" & pid=$!
a=$(window 'Shell test A')
shot "$a" a-1280x720
kill "$pid"; wait "$pid" || :; pid=''
echo 'F2, arrows, state reset, compile failure retention, 1280x720: PASS' | tee "$out/proof.txt"

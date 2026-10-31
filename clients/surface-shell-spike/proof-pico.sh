#!/usr/bin/env bash
# THROWAWAY: exercise the actual Pico package through the runtime shell.
set -euo pipefail
bin=$(realpath "$1")
out=$(realpath -m "$2")
root=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"
display=${SHELL_PROOF_DISPLAY:-:95}
Xvfb "$display" -screen 0 1280x720x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
xvfb=$!
pid=''
temp=$(mktemp -d /tmp/surface-shell-pico-proof.XXXXXX)
cleanup() { [[ -z $pid ]] || kill "$pid" 2>/dev/null || :; kill "$xvfb" 2>/dev/null || :; rm -rf "$temp"; }
trap cleanup EXIT
for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${display#:} ]] && break; sleep .1; done
export DISPLAY=$display SHELL_ROOT=$root SHELL_SURFACES=pico,_test/a SHELL_STATS=1 SLINT_BACKEND=winit-software
unset WAYLAND_DISPLAY
window() {
  local id=''
  for _ in $(seq 100); do
    id=$(xdotool search --onlyvisible --name "^$1$" 2>/dev/null | head -1 || :)
    [[ -n $id ]] && { sleep .5; echo "$id"; return; }
    sleep .1
  done
  echo "no window $1" >&2; return 1
}
shot() { import -window "$1" "$out/$2.png"; }
key() { xdotool windowfocus --sync "$1" key "$2"; sleep .3; }
"$bin" 2>"$out/stats.log" & pid=$!
pico=$(window Pico)
shot "$pico" pico-start
key "$pico" Right
shot "$pico" pico-right
key "$pico" Left
shot "$pico" pico-left
key "$pico" F2
a=$(window 'Shell test A')
shot "$a" test-after-pico
if xdotool search --onlyvisible --name '^Pico$' >/dev/null 2>&1; then echo 'old Pico visible' >&2; exit 1; fi
key "$a" F2
pico=$(window Pico)
shot "$pico" pico-reset
sleep 5
kill "$pid"; wait "$pid" || :; pid=''
compare -metric AE "$out/pico-start.png" "$out/pico-reset.png" null: 2>"$out/reset-difference.txt"
if compare -metric AE "$out/pico-start.png" "$out/pico-right.png" null: 2>"$out/right-difference.txt"; then echo 'right did not change chosen cart' >&2; exit 1; fi
if compare -metric AE "$out/pico-right.png" "$out/pico-left.png" null: 2>"$out/left-difference.txt"; then echo 'left did not change chosen cart' >&2; exit 1; fi
# Derived from the existing fixture: an empty shelf makes stage.cart null.
mkdir -p "$temp/fixtures"
ln -s "$root/surfaces" "$temp/surfaces"
python3 - "$root/fixtures/model.json" "$temp/fixtures/model.json" <<'PY'
import json, sys
model = json.load(open(sys.argv[1]))
model['catalog']['games'] = []
with open(sys.argv[2], 'w') as output:
    json.dump(model, output)
PY
SHELL_ROOT="$temp" "$bin" 2>"$out/empty-stats.log" & pid=$!
pico=$(window Pico)
shot "$pico" pico-empty-cart-null
key "$pico" Right
shot "$pico" pico-empty-after-right
kill -0 "$pid"
kill "$pid"; wait "$pid" || :; pid=''
# The shared fixture includes carts with no coverArtUrl; they render on the shelf.
python3 - "$root/fixtures/model.json" >"$out/missing-art.txt" <<'PY'
import json, sys
model = json.load(open(sys.argv[1]))
games = model['catalog']['games']
missing = [game['id'] for game in games if not game.get('coverArtUrl')]
assert missing, 'fixture must exercise absent art'
print(f'{len(missing)} of {len(games)} games have no coverArtUrl: ' + ', '.join(missing))
PY
if grep -E 'shell: (property|QuickJS)|Error:|panicked' "$out/stats.log" "$out/empty-stats.log"; then exit 1; fi
echo 'Pico load, right/left selection, F2 round trip/reset, missing art, null cart: PASS' | tee "$out/proof.txt"

#!/usr/bin/env bash
# THROWAWAY SPIKE proof under Xvfb: Zone on fixture data, three presses, a layer
# and Back; a .slint edit and a logic.ts edit live (each logic mode), reverted;
# then Pico with one edit of each kind.
#
#   proof.sh KORRI_LIVE_BIN OUT_DIR
#
# Needs Xvfb, xdotool, ImageMagick's import and bun on PATH, and the X11
# libraries on LD_LIBRARY_PATH (the Nix wrapper of korri-live sets its own).
# Edits files in the checkout's packages and restores them byte for byte.
set -euo pipefail
live=$(realpath "$1")
out=$(realpath -m "$2")
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
zone=$root/clients/shell/packages/zone
pico=$root/clients/shell/packages/pico
mkdir -p "$out"
display=:${PROOF_DISPLAY:-88}
work=$(mktemp -d /tmp/korri-live-proof.XXXXXX)
backup=$work/backup
mkdir "$backup"
cp "$zone/ui/tokens.slint" "$zone/logic.ts" "$zone/logic.js" "$backup/"
mkdir "$backup/pico"
cp "$pico/ui/palette.slint" "$pico/logic.ts" "$pico/logic.js" "$backup/pico/"
pids=()
restore() {
  cp "$backup/tokens.slint" "$zone/ui/tokens.slint"
  cp "$backup/logic.ts" "$zone/logic.ts"
  cp "$backup/logic.js" "$zone/logic.js"
  cp "$backup/pico/palette.slint" "$pico/ui/palette.slint"
  cp "$backup/pico/logic.ts" "$pico/logic.ts"
  cp "$backup/pico/logic.js" "$pico/logic.js"
}
cleanup() {
  restore
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || :; done
  wait 2>/dev/null || :
  rm -rf "$work"
}
trap cleanup EXIT

Xvfb "$display" -screen 0 2320x1100x24 -nolisten tcp >"$work/xvfb.log" 2>&1 &
pids+=($!)
for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${display#:} ]] && break; sleep 0.1; done
export DISPLAY=$display
unset WAYLAND_DISPLAY

cd "$root"
"$live" --surface zone --device rg35xxsp >"$out/korri-live.log" 2>&1 &
pids+=($!)
win() { # name regex
  local id=''
  for _ in $(seq 200); do
    id=$(xdotool search --onlyvisible --name "$1" 2>/dev/null | tail -1 || :)
    [[ -n $id ]] && { echo "$id"; return; }
    sleep 0.1
  done
  echo "no window $1" >&2; return 1
}
zid=$(win '^Zone$')
tid=$(win '^korri-live bench$')
# No window manager: put the bench beside the package and draw it whole.
xdotool windowmove "$tid" 1940 0 || :
sleep 0.3
xdotool windowmove "$tid" 1940 0
xdotool windowsize "$tid" 361 760; sleep 0.3; xdotool windowsize "$tid" 360 760
sleep 1.5
bench() { sleep 0.3; xdotool windowsize "$tid" 361 760; sleep 0.2; xdotool windowsize "$tid" 360 760; sleep 0.4; import -window "$tid" "$out/$1.png"; }
shot() { sleep "${2:-0.8}"; import -window "$zid" "$out/$1.png"; }
key() { xdotool windowfocus --sync "$zid" key "$1"; sleep 0.35; }
click() { xdotool mousemove "$1" "$2" click 1; sleep 0.3; }
# The bench at 1940,0: the list's rows are 36 px from y 311; a wheel step scrolls 120 px.
pick_row() { # index, wheel steps
  xdotool mousemove 2120 450
  for _ in $(seq "$2"); do xdotool click 5; sleep 0.1; done
  sleep 0.4
  click 2120 $((311 + 18 + 36 * $1 - 120 * $2))
}
mark() { echo "MARK $(date +%T.%N) $*" >>"$out/marks.txt"; }

# 1. The picker: Zone's `games-gameopts` scenario (preview/scenarios/games-gameopts.json,
#    Zone's ready fixture with Korri's actions for the focused game).
pick_row 9 1
zid=$(win '^Zone$')
shot zone-1-games-model 1.2
bench bench-1-model-picked
# 2. Three presses move the band's focus.
mark presses
key Right; shot zone-2-right-1
key Right; shot zone-3-right-2
key Right; shot zone-4-right-3
# 3. Options opens the GAME OPTS. layer; Back closes it.
key o; shot zone-5-layer-open
key Escape; shot zone-6-back-closed

# 4. A .slint edit: Zone's GAMES ground (ui/tokens.slint) turns pink.
mark slint-edit
sed -i 's/out property <color> ground-games: #4ad39c;/out property <color> ground-games: #d34a9c;/' "$zone/ui/tokens.slint"
shot zone-7-slint-edit 3
key Left; shot zone-8-after-slint-press
# 5. A logic.ts edit (replay): every fact label gets a "!".
mark logic-edit-replay
python3 - "$zone/logic.ts" <<'PY'
import sys; p = sys.argv[1]; s = open(p).read()
old = 'lastView = { state, model, view: zoneView(state.zone, model, screenOf(state)) }'
new = 'const made = zoneView(state.zone, model, screenOf(state)); lastView = { state, model, view: { ...made, facts: made.facts.map(f => ({ ...f, label: f.label + "!" })) } as typeof made }'
assert old in s; open(p, "w").write(s.replace(old, new))
PY
shot zone-9-logic-edit-replay 5
key Right; shot zone-10-after-logic-press
bench bench-2-after-edits
# 6. Revert both (each is a save: two more live reloads).
mark revert
cp "$backup/tokens.slint" "$zone/ui/tokens.slint"
shot zone-11-slint-reverted 3
cp "$backup/logic.ts" "$zone/logic.ts"
shot zone-12-logic-reverted 5
# 7. The same logic edit with transfer, then fresh.
for mode in transfer fresh; do
  case $mode in transfer) click 2120 251 ;; fresh) click 2234 251 ;; esac
  key Right
  mark "logic-edit-$mode"
  python3 - "$zone/logic.ts" <<'PY'
import sys; p = sys.argv[1]; s = open(p).read()
old = 'lastView = { state, model, view: zoneView(state.zone, model, screenOf(state)) }'
new = 'const made = zoneView(state.zone, model, screenOf(state)); lastView = { state, model, view: { ...made, facts: made.facts.map(f => ({ ...f, label: f.label + "!" })) } as typeof made }'
assert old in s; open(p, "w").write(s.replace(old, new))
PY
  shot "zone-13-logic-edit-$mode" 5
  cp "$backup/logic.ts" "$zone/logic.ts"
  sleep 5
done
bench bench-3-modes
click 2005 251   # back to replay

# 8. Pico: F2 (the shell's next-surface key) opens it in the same window.
mark pico
key F2
sleep 3
zid=$(win '^Pico$')
shot pico-1-open 1
key Right; shot pico-2-right-1
key Right; shot pico-3-right-2
key Right; shot pico-4-right-3
key o; shot pico-5-find-open
key Escape; shot pico-6-back-closed
key Right
mark pico-slint-edit
python3 - "$pico/ui/palette.slint" <<'PY'
import sys, re; p = sys.argv[1]; s = open(p).read()
s2 = re.sub(r'(out property <color> black: )#[0-9a-fA-F]{6}', r'\1#1d2b53', s, count=1)
assert s2 != s, "no black in palette"; open(p, "w").write(s2)
PY
shot pico-7-slint-edit 3
mark pico-logic-edit
python3 - "$pico/logic.ts" <<'PY'
import sys; p = sys.argv[1]; s = open(p).read()
old = 'view: (state: State, model: KorriModel) => view(state, model),'
new = 'view: (state: State, model: KorriModel) => ({ ...view(state, model), fontFamily: "Pico monogram" }),'
assert old in s; open(p, "w").write(s.replace(old, new))
PY
shot pico-8-logic-edit 5
bench bench-4-pico
mark done
restore
sleep 4
grep -E "LIVE_RELOAD|logic reload|refused|violation" "$out/korri-live.log" >"$out/reloads.txt" || :
echo "proof in $out"

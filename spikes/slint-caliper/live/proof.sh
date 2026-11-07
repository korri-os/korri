#!/usr/bin/env bash
# THROWAWAY SPIKE proof under Xvfb: Zone on fixture data, the model picker,
# three presses, a layer and Back, both device sizes; a .slint edit and a
# logic.ts edit live (each logic mode), reverted; timing loops measured in the
# process and from the X server's pixels; then Pico with one edit of each kind.
#
#   proof.sh KORRI_LIVE_BIN OUT_DIR        (run inside a Korri checkout)
#
# Needs Xvfb, xdotool, xwd, ImageMagick's import, python3 and bun on PATH
# (`nix run .#slint-caliper-live-proof` sets them). Edits files in the
# checkout's packages and restores them byte for byte, also on failure.
set -euo pipefail
live=$(realpath "$1")
out=$(realpath -m "$2")
here=$(cd "$(dirname "$0")" && pwd)
root=$PWD
while [[ $root != / && ! -d $root/clients/shell/packages ]]; do root=$(dirname "$root"); done
[[ -d $root/clients/shell/packages ]] || { echo "run inside a Korri checkout" >&2; exit 2; }
zone=$root/clients/shell/packages/zone
pico=$root/clients/shell/packages/pico
mkdir -p "$out"
: >"$out/marks.txt"
: >"$out/probes.txt"
display=:${PROOF_DISPLAY:-88}
work=$(mktemp -d /tmp/korri-live-proof.XXXXXX)
backup=$work/backup
mkdir -p "$backup/pico"
cp "$zone/ui/tokens.slint" "$zone/logic.ts" "$zone/logic.js" "$backup/"
cp "$pico/ui/palette.slint" "$pico/logic.ts" "$pico/logic.js" "$backup/pico/"
pids=()
restore() {
  cmp -s "$backup/tokens.slint" "$zone/ui/tokens.slint" || cp "$backup/tokens.slint" "$zone/ui/tokens.slint"
  cmp -s "$backup/logic.ts" "$zone/logic.ts" || cp "$backup/logic.ts" "$zone/logic.ts"
  cmp -s "$backup/logic.js" "$zone/logic.js" || cp "$backup/logic.js" "$zone/logic.js"
  cmp -s "$backup/pico/palette.slint" "$pico/ui/palette.slint" || cp "$backup/pico/palette.slint" "$pico/ui/palette.slint"
  cmp -s "$backup/pico/logic.ts" "$pico/logic.ts" || cp "$backup/pico/logic.ts" "$pico/logic.ts"
  cmp -s "$backup/pico/logic.js" "$pico/logic.js" || cp "$backup/pico/logic.js" "$pico/logic.js"
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
win() { # title regex
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
# The bench at 1940,0: the model list's rows are 36 px from y 311; a wheel step scrolls 120 px.
pick_row() { # index, wheel steps
  xdotool mousemove 2120 450
  for _ in $(seq "$2"); do xdotool click 5; sleep 0.1; done
  sleep 0.4
  click 2120 $((311 + 18 + 36 * $1 - 120 * $2))
}
mark() { echo "MARK $(date +%T.%N) $*" >>"$out/marks.txt"; }
# probe LABEL FILE X Y W H -- COMMAND...: run COMMAND (the edit) while probe.py watches.
probe() {
  local label=$1 file=$2 x=$3 y=$4 w=$5 h=$6; shift 7
  python3 "$here/probe.py" "$zid" "$x" "$y" "$w" "$h" "$file" "${PROBE_SECONDS:-3}" >"$work/probe" &
  local p=$!
  sleep 0.4
  "$@"
  wait "$p" || :
  echo "$label $(cat "$work/probe")" | tee -a "$out/probes.txt"
}
zone_pink() { sed -i 's/out property <color> ground-games: #4ad39c;/out property <color> ground-games: #d34a9c;/' "$zone/ui/tokens.slint"; }
zone_green() { cp "$backup/tokens.slint" "$zone/ui/tokens.slint"; }
zone_bang() {
  python3 - "$zone/logic.ts" <<'PY'
import sys; p = sys.argv[1]; s = open(p).read()
old = 'lastView = { state, model, view: zoneView(state.zone, model, screenOf(state)) }'
new = 'const made = zoneView(state.zone, model, screenOf(state)); lastView = { state, model, view: { ...made, facts: made.facts.map(f => ({ ...f, label: f.label + "!" })) } as typeof made }'
assert old in s; open(p, "w").write(s.replace(old, new))
PY
}
zone_unbang() { cp "$backup/logic.ts" "$zone/logic.ts"; }
# Probe regions in the 640x480 window: the ground at the left edge (.slint) and
# the facts row and its neighbours left of the ring (logic.ts).
GROUND=(0 190 8 20)
FACTS=(20 290 370 100)

# 0. Zone opens on the shell's fixture model (clients/shell/fixtures/model.json).
shot zone-0-shell-fixture 1
# 1. The picker sends a model to the running fixture korrid as `model FILE`: first Zone's
#    `games-empty` (preview/scenarios/games-empty.json), then `games-gameopts`
#    (Zone's ready fixture with Korri's actions for the focused game).
mark pick-model
pick_row 4 0
shot zone-1a-games-empty 1.2
bench bench-0-empty-picked
pick_row 9 1
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
probe slint-edit "$zone/ui/tokens.slint" "${GROUND[@]}" -- zone_pink
shot zone-7-slint-edit 0.5
key Left; shot zone-8-after-slint-press
# 5. A logic.ts edit (replay): every fact label gets a "!".
mark logic-edit-replay
probe logic-edit-replay "$zone/logic.ts" "${FACTS[@]}" -- zone_bang
shot zone-9-logic-edit-replay 0.5
key Right; shot zone-10-after-logic-press
bench bench-2-after-edits
# 6. Revert both (each is a save: two more live reloads).
mark revert
probe slint-revert "$zone/ui/tokens.slint" "${GROUND[@]}" -- zone_green
shot zone-11-slint-reverted 0.5
probe logic-revert "$zone/logic.ts" "${FACTS[@]}" -- zone_unbang
shot zone-12-logic-reverted 0.5
# 7. The same logic edit with transfer, then fresh.
for mode in transfer fresh; do
  case $mode in transfer) click 2120 251 ;; fresh) click 2234 251 ;; esac
  key Right
  mark "logic-edit-$mode"
  shot "zone-13-before-$mode" 0.5
  sleep 1
  probe "logic-edit-$mode" "$zone/logic.ts" "${FACTS[@]}" -- zone_bang
  shot "zone-14-logic-edit-$mode" 0.5
  zone_unbang
  sleep 3
done
bench bench-3-modes
click 2005 251   # back to replay
# 8. Timing: five more saves of each kind, each way (replay).
mark timing
# Up then Down before each save keeps Zone out of its idle attract (30 s, src/state/navigation.ts).
awake() { key Up; key Down; sleep 1; }
for i in 1 2 3 4 5; do
  awake
  probe "slint-$i-on" "$zone/ui/tokens.slint" "${GROUND[@]}" -- zone_pink
  probe "slint-$i-off" "$zone/ui/tokens.slint" "${GROUND[@]}" -- zone_green
  awake
  probe "logic-$i-on" "$zone/logic.ts" "${FACTS[@]}" -- zone_bang
  probe "logic-$i-off" "$zone/logic.ts" "${FACTS[@]}" -- zone_unbang
done
# 8b. Broken saves: each is refused, the package keeps running as it was, then reverted.
mark broken
awake
echo 'component Broken {' >>"$zone/ui/tokens.slint"
sleep 3
shot zone-18-broken-slint 0
zone_green
sleep 3
echo 'export const broken = (' >>"$zone/logic.ts"
sleep 2
shot zone-19-broken-logic 0
key Right; shot zone-20-after-broken-press
zone_unbang
sleep 2
# 9. A long journal: 300 more presses, then a logic edit replays all of them.
mark long-journal
xdotool windowfocus --sync "$zid"
xdotool key --delay 30 --repeat 150 Right Left
sleep 1
probe logic-long-journal "$zone/logic.ts" "${FACTS[@]}" -- zone_bang
shot zone-15-long-journal 0.5
zone_unbang
sleep 3
# 10. The device size: odin2portal (1920x1080), then back to rg35xxsp.
mark device
click 2034 152
sleep 1.5
shot zone-16-odin2portal 0.5
click 2034 116
sleep 1.5
shot zone-17-rg35xxsp 0.5

# 11. Pico: F2 (the shell's next-surface key) opens it in the same window.
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
pico_navy() {
  python3 - "$pico/ui/palette.slint" <<'PY'
import sys, re; p = sys.argv[1]; s = open(p).read()
s2 = re.sub(r'(out property <color> black: )#[0-9a-fA-F]{6}', r'\1#1d2b53', s, count=1)
assert s2 != s, "no black in palette"; open(p, "w").write(s2)
PY
}
pico_font() {
  python3 - "$pico/logic.ts" <<'PY'
import sys; p = sys.argv[1]; s = open(p).read()
old = 'view: (state: State, model: KorriModel) => view(state, model),'
new = 'view: (state: State, model: KorriModel) => ({ ...view(state, model), fontFamily: "Pico monogram" }),'
assert old in s; open(p, "w").write(s.replace(old, new))
PY
}
probe pico-slint-edit "$pico/ui/palette.slint" 590 190 40 20 -- pico_navy
shot pico-7-slint-edit 0.5
key Right; shot pico-8-after-slint-press
mark pico-logic-edit
probe pico-logic-edit "$pico/logic.ts" 0 0 640 40 -- pico_font
shot pico-9-logic-edit 0.5
key Right; shot pico-10-after-logic-press
cp "$backup/pico/logic.ts" "$pico/logic.ts"
sleep 3
mark pico-logic-transfer
click 2120 251
key Right
shot pico-11-before-transfer 0.5
probe pico-logic-transfer "$pico/logic.ts" 0 0 640 40 -- pico_font
shot pico-12-logic-transfer 0.5
bench bench-4-pico
mark done
restore
sleep 4
grep -E "LIVE_RELOAD|LIVE_RELOAD_SUPERSEDED|logic reload|refused|violation|transpile failed" "$out/korri-live.log" >"$out/reloads.txt" || :
echo "proof in $out"

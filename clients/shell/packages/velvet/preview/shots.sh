#!/usr/bin/env bash
# Velvet's screens through the real shell under Xvfb, over the fixture korrid.
#
#   preview/shots.sh SHELL FIXTURE_KORRID OUT_DIR [SIZES] [SCENARIOS...]
#
# SIZES is a comma list of WxH (default: the four the design names,
# 1920x1080,1240x1080,640x480,480x800). Each scenario
# (preview/scenarios/<name>.json, written by preview/scenarios.ts) starts a
# fresh fixture korrid with its first model, starts the shell on Velvet at
# the size, plays the steps as key presses and screenshots the window:
# OUT_DIR/<WxH>/<name>.png. The fixture korrid answers MoveCatalog (its
# window, the `systemId` filter, `systems`) and FocusGame as korrid does, so
# what a shot shows went through the treaty.
#
# Steps: up down left right confirm back menu options system, `model N`
# (publish the scenario's Nth model), `wait SECONDS`.
#
# Needs Xvfb, xdotool and ImageMagick on PATH and the X11 runtime libraries
# on LD_LIBRARY_PATH (clients/shell/default.nix's shell-proof sets both).
set -euo pipefail
shell=$(realpath "$1")
fixture=$(realpath "$2")
out=$(realpath -m "$3")
IFS=, read -r -a sizes <<<"${4:-1920x1080,1240x1080,640x480,480x800}"
shift $(( $# < 4 ? $# : 4 ))
here=$(cd "$(dirname "$0")" && pwd)
packages=$(cd "$here/../.." && pwd)
if (( $# > 0 )); then names=("$@"); else names=(); for f in "$here"/scenarios/*.json; do names+=("$(basename "$f" .json)"); done; fi
work=$(mktemp -d /tmp/velvet-shots.XXXXXX)
display=${SHOTS_DISPLAY:-:94}
pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || :; done
  wait 2>/dev/null || :
  rm -rf "$work"
}
trap cleanup EXIT
Xvfb "$display" -screen 0 2000x1200x24 -nolisten tcp >"$work/xvfb.log" 2>&1 &
pids+=($!)
for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${display#:} ]] && break; sleep 0.1; done
export DISPLAY=$display
unset WAYLAND_DISPLAY
mkdir -m 700 "$work/cache"
# Install once: verify every package into the cache.
"$shell" install --packages "$packages" --cache "$work/cache" >"$work/install.log"

key_of() {
  case $1 in
    up) echo Up ;; down) echo Down ;; left) echo Left ;; right) echo Right ;;
    confirm) echo Return ;; back) echo Escape ;; menu) echo m ;; options) echo o ;; system) echo s ;;
    *) echo "unknown step $1" >&2; return 1 ;;
  esac
}

shoot() { # size name
  local size=$1 name=$2 scenario="$here/scenarios/$2.json" run="$work/run"
  rm -rf "$run"; mkdir -m 700 "$run" "$run/credentials"
  local count i
  count=$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["models"]))' "$scenario")
  for (( i = 0; i < count; i++ )); do
    python3 -c 'import json,sys; json.dump(json.load(open(sys.argv[1]))["models"][int(sys.argv[2])], open(sys.argv[3], "w"))' "$scenario" "$i" "$run/model-$i.json"
  done
  mkfifo "$run/control"
  "$fixture" --games 12 --credentials "$run/credentials" <"$run/control" >"$run/fixture.out" 2>"$run/fixture.log" &
  local fixture_pid=$!
  exec 7>"$run/control"
  for _ in $(seq 100); do grep -q KORRID_SHELL_ORIGIN "$run/fixture.out" 2>/dev/null && break; sleep 0.05; done
  echo "model $run/model-0.json" >&7
  sleep 0.2
  KORRID_ADDRESS=$(sed -n 's/^KORRID_ADDRESS=//p' "$run/fixture.out") \
  KORRID_SHELL_ORIGIN=$(sed -n 's/^KORRID_SHELL_ORIGIN=//p' "$run/fixture.out") \
  CREDENTIALS_DIRECTORY=$run/credentials \
    "$shell" --packages "$packages" --cache "$work/cache" --surfaces velvet --placement "$size" 2>"$run/shell.log" &
  local shell_pid=$!
  local id='' title=Velvet
  python3 -c 'import json,sys; m=json.load(open(sys.argv[1]))["models"][0]; sys.exit(0 if m.get("presentation",{}).get("kind")=="gameplay-overlay" else 1)' "$scenario" && title="Velvet overlay"
  for _ in $(seq 300); do
    id=$(xdotool search --onlyvisible --name "^$title\$" 2>/dev/null | tail -1 || :)
    [[ -n $id ]] && break
    sleep 0.1
  done
  if [[ -z $id ]]; then echo "no window for $name ($size)" >&2; cat "$run/shell.log" >&2; kill "$shell_pid" "$fixture_pid" 2>/dev/null || :; exec 7>&-; return 0; fi
  # Home asks Korri for its lists one after another before it settles.
  sleep 1.2
  while read -r step; do
    case $step in
      "model "*) echo "model $run/model-${step#model }.json" >&7; sleep 0.6 ;;
      "wait "*) sleep "${step#wait }" ;;
      *) xdotool windowfocus --sync "$id" key "$(key_of "$step")"; sleep 0.35 ;;
    esac
  done < <(python3 -c 'import json,sys; [print(s) for s in json.load(open(sys.argv[1]))["steps"]]' "$scenario")
  # The focus rests: the facts come in after the dwell, and every motion ends.
  sleep 1.2
  mkdir -p "$out/$size"
  id=$(xdotool search --onlyvisible --name "^Velvet" 2>/dev/null | tail -1 || echo "$id")
  import -window "$id" "$out/$size/$name.png"
  echo requests >&7
  sleep 0.2
  grep "^{" "$run/fixture.out" >"$out/$size/$name.requests.txt" || :
  kill "$shell_pid" 2>/dev/null || :
  wait "$shell_pid" 2>/dev/null || :
  exec 7>&-
  kill "$fixture_pid" 2>/dev/null || :
  wait "$fixture_pid" 2>/dev/null || :
}

for size in "${sizes[@]}"; do
  for name in "${names[@]}"; do
    shoot "$size" "$name"
    echo "shot $size $name"
  done
done

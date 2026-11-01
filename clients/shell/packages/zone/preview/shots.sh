#!/usr/bin/env bash
# Zone's pages through the real shell under Xvfb, over the fixture korrid.
#
#   preview/shots.sh SHELL FIXTURE_KORRID OUT_DIR [LOOKS] [SIZES] [SCENARIOS...]
#
# LOOKS is a comma list (modern,blend,retro), SIZES a comma list of WxH
# (640x480,1280x720). Each scenario (preview/scenarios/<name>.json, written
# by preview/scenarios.ts from Zone's own fixtures) starts a fresh fixture
# korrid with its first model and the look stored as `zone.look`, starts the
# shell on Zone at the size, plays the steps and screenshots the window:
# OUT_DIR/<look>/<name>.<WxH>.png.
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
IFS=, read -r -a looks <<<"${4:-modern,blend,retro}"
IFS=, read -r -a sizes <<<"${5:-640x480,1280x720}"
shift $(( $# < 5 ? $# : 5 ))
here=$(cd "$(dirname "$0")" && pwd)
packages=$(cd "$here/../.." && pwd)
if (( $# > 0 )); then names=("$@"); else names=(); for f in "$here"/scenarios/*.json; do names+=("$(basename "$f" .json)"); done; fi
work=$(mktemp -d /tmp/zone-shots.XXXXXX)
display=${SHOTS_DISPLAY:-:93}
pids=()
cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || :; done
  wait 2>/dev/null || :
  rm -rf "$work"
}
trap cleanup EXIT
Xvfb "$display" -screen 0 1400x800x24 -nolisten tcp >"$work/xvfb.log" 2>&1 &
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

shoot() { # look size name
  local look=$1 size=$2 name=$3 scenario="$here/scenarios/$3.json" run="$work/run"
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
  for _ in $(seq 100); do grep -q KORRID_PORTAL_ORIGIN "$run/fixture.out" 2>/dev/null && break; sleep 0.05; done
  echo "model $run/model-0.json" >&7
  echo "store zone zone.look $look" >&7
  sleep 0.2
  KORRID_ADDRESS=$(sed -n 's/^KORRID_ADDRESS=//p' "$run/fixture.out") \
  KORRID_PORTAL_ORIGIN=$(sed -n 's/^KORRID_PORTAL_ORIGIN=//p' "$run/fixture.out") \
  CREDENTIALS_DIRECTORY=$run/credentials \
    "$shell" --packages "$packages" --cache "$work/cache" --surfaces zone --placement "$size" 2>"$run/shell.log" &
  local shell_pid=$!
  local id='' title=Zone
  python3 -c 'import json,sys; m=json.load(open(sys.argv[1]))["models"][0]; sys.exit(0 if m.get("presentation",{}).get("kind")=="gameplay-overlay" else 1)' "$scenario" && title="Zone overlay"
  for _ in $(seq 300); do
    id=$(xdotool search --onlyvisible --name "^$title\$" 2>/dev/null | tail -1 || :)
    [[ -n $id ]] && break
    sleep 0.1
  done
  if [[ -z $id ]]; then echo "no window for $name ($look $size)" >&2; cat "$run/shell.log" >&2; kill "$shell_pid" "$fixture_pid" 2>/dev/null || :; exec 7>&-; return 0; fi
  sleep 1
  while read -r step; do
    case $step in
      "model "*) echo "model $run/model-${step#model }.json" >&7; sleep 0.6 ;;
      "wait "*) sleep "${step#wait }" ;;
      *) xdotool windowfocus --sync "$id" key "$(key_of "$step")"; sleep 0.25 ;;
    esac
  done < <(python3 -c 'import json,sys; [print(s) for s in json.load(open(sys.argv[1]))["steps"]]' "$scenario")
  sleep 0.8
  mkdir -p "$out/$look"
  id=$(xdotool search --onlyvisible --name "^Zone" 2>/dev/null | tail -1 || echo "$id")
  import -window "$id" "$out/$look/$name.$size.png"
  echo requests >&7
  sleep 0.2
  grep "^{" "$run/fixture.out" >"$out/$look/$name.$size.requests.txt" || :
  kill "$shell_pid" 2>/dev/null || :
  wait "$shell_pid" 2>/dev/null || :
  exec 7>&-
  kill "$fixture_pid" 2>/dev/null || :
  wait "$fixture_pid" 2>/dev/null || :
}

for look in "${looks[@]}"; do
  for size in "${sizes[@]}"; do
    for name in "${names[@]}"; do
      shoot "$look" "$size" "$name"
      echo "shot $look $size $name"
    done
  done
done

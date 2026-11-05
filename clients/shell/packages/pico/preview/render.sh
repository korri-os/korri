#!/usr/bin/env bash
# Render the preview scenarios with the real shell under Xvfb, over the
# fixture korrid, at Pico's Caliper devices (surfaces/pico/vite.config.ts).
#
#   preview/render.sh SHELL FIXTURE_KORRID GEN_DIR OUT_DIR [DEVICE...]
#
# GEN_DIR is `bun preview/scenarios.ts GEN_DIR`. Needs Xvfb, xdotool and
# ImageMagick on PATH, and the X11 runtime libraries on LD_LIBRARY_PATH (as
# clients/shell's `nix run .#shell-proof` sets them). Writes
# OUT_DIR/<scenario>.<device>.png. ONLY="name name" renders just those.
set -euo pipefail
shell=$(realpath "$1")
fixture=$(realpath "$2")
gen=$(realpath "$3")
out=$(realpath -m "$4")
shift 4
devices=("$@")
[[ ${#devices[@]} -gt 0 ]] || devices=(rg353m odin2portal rpminiv2 r36tmax rgds rgds-side)
packages=$(cd "$(dirname "$0")/../.." && pwd)
mkdir -p "$out"
work=$(mktemp -d /tmp/pico-preview.XXXXXX)
trap 'rm -rf "$work"' EXIT

size_of() {
  case $1 in
    rg353m) echo 640x480 ;; odin2portal) echo 1920x1080 ;; rpminiv2) echo 1240x1080 ;;
    r36tmax) echo 720x720 ;; rgds) echo 640x960 ;; rgds-side) echo 1280x480 ;;
  esac
}

mkdir -m 700 "$work/cache"
# Install time: verify the packages into the cache once.
"$shell" install --packages "$packages" --cache "$work/cache" >"$work/install.log" 2>&1 || { cat "$work/install.log"; exit 1; }

render_device() {
  local device=$1 index=$2 size w h display
  size=$(size_of "$device")
  w=${size%x*}
  h=${size#*x}
  display=":$((120 + index))"
  local dir="$work/$device"
  mkdir -p "$dir"
  Xvfb "$display" -screen 0 "${w}x${h}x24" -nolisten tcp >"$dir/xvfb.log" 2>&1 &
  local xvfb=$!
  for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${display#:} ]] && break; sleep 0.1; done
  local count name
  count=$(jq length "$gen/scenarios.json")
  for i in $(seq 0 $((count - 1))); do
    name=$(jq -r ".[$i].name" "$gen/scenarios.json")
    # ONLY="a b" renders just those scenarios.
    [[ -z ${ONLY:-} || " $ONLY " == *" $name "* ]] || continue
    local displays
    displays=$(jq -c ".[$i].displays" "$gen/scenarios.json")
    if [[ $displays == null ]]; then
      displays="[{\"id\":\"$device\",\"main\":true,\"x\":0,\"y\":0,\"width\":$w,\"height\":$h,\"touch\":false}]"
    elif [[ $device != rgds && $device != rgds-side ]]; then
      continue
    elif [[ $device == rgds-side ]]; then
      displays='[{"id":"top","main":true,"x":0,"y":0,"width":640,"height":480,"touch":false},{"id":"bottom","main":false,"x":640,"y":0,"width":640,"height":480,"touch":true}]'
    fi
    mkdir -m 700 -p "$dir/cred-$i"
    rm -f "$dir/control"
    mkfifo "$dir/control"
    "$fixture" --games 0 --credentials "$dir/cred-$i" --model "$gen/$name.json" --displays "$displays" \
      <"$dir/control" >"$dir/fixture.out" 2>"$dir/fixture.log" &
    local fixture_pid=$!
    exec 7>"$dir/control"
    for _ in $(seq 100); do grep -q KORRID_SHELL_ORIGIN "$dir/fixture.out" 2>/dev/null && break; sleep 0.05; done
    (
      export DISPLAY=$display
      unset WAYLAND_DISPLAY
      export KORRID_ADDRESS KORRID_SHELL_ORIGIN CREDENTIALS_DIRECTORY=$dir/cred-$i
      KORRID_ADDRESS=$(sed -n 's/^KORRID_ADDRESS=//p' "$dir/fixture.out")
      KORRID_SHELL_ORIGIN=$(sed -n 's/^KORRID_SHELL_ORIGIN=//p' "$dir/fixture.out")
      "$shell" --packages "$packages" --cache "$work/cache" --surfaces pico --placement "${w}x${h}" \
        >"$dir/shell.log" 2>&1 &
      local shell_pid=$!
      local id=''
      for _ in $(seq 200); do
        id=$(xdotool search --onlyvisible --name '^Pico( overlay)?$' 2>/dev/null | tail -1 || :)
        [[ -n $id ]] && break
        sleep 0.1
      done
      sleep 1.2
      for key in $(jq -r ".[$i].keys[]" "$gen/scenarios.json"); do
        xdotool windowfocus --sync "$id" key "$key" 2>/dev/null || :
        sleep 0.3
      done
      for file in $(jq -r ".[$i].then[]" "$gen/scenarios.json"); do
        echo "model $file" >&7
        sleep 0.8
      done
      sleep 1
      id=$(xdotool search --onlyvisible --name '^Pico( overlay)?$' 2>/dev/null | tail -1 || :)
      import -display "$display" -window "$id" "$out/$name.$device.png" 2>/dev/null || echo "no shot $name $device" >&2
      kill "$shell_pid" 2>/dev/null || :
      wait "$shell_pid" 2>/dev/null || :
    )
    exec 7>&-
    kill "$fixture_pid" 2>/dev/null || :
    wait "$fixture_pid" 2>/dev/null || :
  done
  kill "$xvfb" 2>/dev/null || :
}

index=0
for device in "${devices[@]}"; do
  render_device "$device" "$index" &
  index=$((index + 1))
done
wait
echo "rendered into $out"

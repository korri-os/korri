# PID 1 runs this as the gameplay uid. The game receives only this launch's
# Pulse socket, never the shared Pulse or native PipeWire socket, and never
# this helper's runtime directory.
set -euo pipefail
operation=$1
launch=$2
[[ "$launch" =~ ^[0-9a-f]{32}$ ]] || exit 2
sink="korri-game-$launch"
state="/run/korri-game-audio-$launch"
# The shared server's own module list is the only authority for which modules
# belong to this launch. No file a launch could write decides what is unloaded.
# A launch cannot load modules on the shared server: it never sees its socket.
launch_modules() {
  local kind=$1
  pactl list modules short | awk -F '\t' -v kind="$kind" \
    -v sink="sink_name=$sink" -v monitor="source=$sink.monitor" '
      {
        want = ""
        if (kind == "loopback" && $2 == "module-loopback") want = monitor
        if (kind == "sink" && $2 == "module-null-sink") want = sink
        if (want == "") next
        count = split($3, words, " ")
        for (i = 1; i <= count; i++) if (words[i] == want) { print $1; next }
      }'
}
cleanup() {
  local kind id failed=0
  # The loopback reads the sink's monitor, so it goes first.
  for kind in loopback sink; do
    for id in $(launch_modules "$kind"); do
      pactl unload-module "$id" || failed=1
    done
  done
  return "$failed"
}
case "$operation" in
  serve)
    trap 'cleanup' ERR
    pactl load-module module-null-sink "sink_name=$sink" \
      'sink_properties=node.description=Korri-game node.virtual=true priority.session=0' > /dev/null
    pactl set-sink-mute "$sink" true
    sed "s/@LAUNCH_ID@/$launch/g" "${KORRI_GAME_AUDIO_CONFIG:?native game audio config is required}" > "$state/pipewire-pulse.conf"
    exec pipewire-pulse -c "$state/pipewire-pulse.conf"
    ;;
  ready)
    for _ in $(seq 1 100); do
      if timeout 0.25s env PULSE_SERVER="unix:$state/native" pactl info >/dev/null 2>&1; then
        exit 0
      fi
      sleep 0.01
    done
    echo "launch Pulse socket did not become ready" >&2
    exit 1
    ;;
  release)
    # Nothing links the launch to an audible output before this host action.
    # Even a client unmuting its own sink cannot bypass the first-frame gate.
    # The mute also discards the monitor's samples, so none is replayed later.
    pactl set-sink-mute "$sink" false
    if [[ -z "$(launch_modules loopback)" ]]; then
      output=$(pactl get-default-sink)
      if [[ -z "$output" || "$output" == "$sink" ]]; then
        echo "no audible default sink for the launch" >&2
        exit 1
      fi
      pactl load-module module-loopback "source=$sink.monitor" "sink=$output" \
        source_dont_move=true sink_dont_move=true > /dev/null
    fi
    ;;
  stop) cleanup ;;
  *) exit 2 ;;
esac

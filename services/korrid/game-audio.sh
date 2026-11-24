# PID 1 runs this as the gameplay uid. The game receives only this launch's
# Pulse socket, never the shared Pulse or native PipeWire socket.
set -euo pipefail
operation=$1
launch=$2
[[ "$launch" =~ ^[0-9a-f]{32}$ ]] || exit 2
sink="korri-game-$launch"
state="/run/korri-game-audio-$launch"
cleanup() {
  local file id failed=0
  for file in loopback sink; do
    if [[ -s "$state/$file" ]]; then
      read -r id < "$state/$file"
      [[ "$id" =~ ^[0-9]+$ ]] || return 1
      if ! pactl unload-module "$id"; then
        failed=1
        continue
      fi
    fi
    rm -f "$state/$file"
  done
  return "$failed"
}
case "$operation" in
  serve)
    trap 'cleanup' ERR
    pactl get-default-sink > "$state/output"
    pactl load-module module-null-sink "sink_name=$sink" \
      'sink_properties=node.description=Korri-game node.virtual=true priority.session=0' > "$state/sink"
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
    if [[ ! -s "$state/loopback" ]]; then
      read -r output < "$state/output"
      pactl load-module module-loopback "source=$sink.monitor" "sink=$output" \
        source_dont_move=true sink_dont_move=true > "$state/loopback"
    fi
    ;;
  stop) cleanup ;;
  *) exit 2 ;;
esac

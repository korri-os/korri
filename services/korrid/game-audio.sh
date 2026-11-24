# One launch's Pulse sink. PID 1 runs this as the gameplay uid, never korrid
# or root. Sink mute discards samples, including its monitor, before loopback
# reaches the ordinary output that Sunshine records.
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
  prepare)
    output=$(pactl get-default-sink)
    trap 'cleanup' ERR
    pactl load-module module-null-sink "sink_name=$sink" \
      'sink_properties=node.description=Korri-game node.virtual=true priority.session=0' > "$state/sink"
    pactl set-sink-mute "$sink" true
    pactl load-module module-loopback "source=$sink.monitor" "sink=$output" \
      source_dont_move=true sink_dont_move=true > "$state/loopback"
    ;;
  release) pactl set-sink-mute "$sink" false ;;
  stop) cleanup ;;
  *) exit 2 ;;
esac

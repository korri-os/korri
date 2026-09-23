set -eu

runtime_dir=/run/user/1000
sway_socket=/run/korri-compositor/sway-ipc.sock
wayland_display=korri-wayland

attempt=0
while [ ! -S "$runtime_dir/$wayland_display" ] || [ ! -S "$sway_socket" ]; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 150 ]; then
    echo "Korri compositor did not become ready" >&2
    exit 1
  fi
  @coreutils@/bin/sleep 0.1
done

attempt=0
while :; do
  if [ -S "$runtime_dir/pulse/native" ] &&
    @coreutils@/bin/timeout 1 @pipewire@/bin/pw-metadata -n default |
      @gnugrep@/bin/grep -q 'Found "default" metadata'; then
    break
  fi
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 20 ]; then
    echo "Gameplay audio server did not become ready" >&2
    exit 1
  fi
  @coreutils@/bin/sleep 0.25
done

outputs=$(@sway@/bin/swaymsg -s "$sway_socket" -t get_outputs -r)
output_name=$(printf '%s' "$outputs" | @jq@/bin/jq -er 'first(.[] | select(.active == true) | .name)')
output_width=$(printf '%s' "$outputs" | @jq@/bin/jq -er 'first(.[] | select(.active == true) | .current_mode.width)')
output_height=$(printf '%s' "$outputs" | @jq@/bin/jq -er 'first(.[] | select(.active == true) | .current_mode.height)')

export KORRI_COMPOSITOR_OUTPUT_NAME="$output_name"
export KORRI_COMPOSITOR_OUTPUT_WIDTH="$output_width"
export KORRI_COMPOSITOR_OUTPUT_HEIGHT="$output_height"
export SWAYSOCK="$sway_socket"
export WAYLAND_DISPLAY="$wayland_display"
export DISPLAY=:0
export XDG_RUNTIME_DIR="$runtime_dir"
export XDG_SESSION_TYPE=wayland
export HOME=/home/korri
export XDG_CONFIG_HOME=/home/korri/.config
export PULSE_SERVER="unix:$runtime_dir/pulse/native"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime_dir/bus"

@coreutils@/bin/mkdir -p /home/korri/.config/sunshine
exec @sunshine@/bin/sunshine /home/korri/.config/sunshine/sunshine.conf log_path=/dev/null capture=kms encoder=auto

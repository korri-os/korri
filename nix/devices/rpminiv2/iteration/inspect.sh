#!/usr/bin/env nix-shell
#!nix-shell -i bash -p bash coreutils systemd
# Read-only witness from inside the kiosk's mount namespace and account.
set -euo pipefail
[[ "$(id -un)" == korri-portal ]]
for node in /dev/input/js*; do
  name=${node##*/}
  printf 'NODE %s parent=%s\n' "$name" "$(readlink -f "/sys/class/input/$name/device")"
  for attr in name phys id/vendor id/product; do
    printf '  %s=' "$attr"
    cat "/sys/class/input/$name/device/$attr" 2>/dev/null || echo unavailable
  done
  udevadm info --query=property --name="$node" 2>/dev/null | grep -E '^(DEVPATH|ID_INPUT_JOYSTICK)=' || true
done

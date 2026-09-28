#!/usr/bin/env nix-shell
#!nix-shell -i bash -p bash coreutils util-linux systemd gnugrep curl
# Retain only a passing per-boot input view, never the diagnostic executable.
set -euo pipefail
stage=${1:?stage}
expected=${2:?approved system}
[[ "$stage" =~ ^/run/korri-input-trial\.[A-Za-z0-9]+$ && "$(stat -c '%u' "$stage")" == 0 ]]
[[ "$(tr -d '\0' </proc/device-tree/model)" == 'Retroid Pocket Mini V2' ]]
[[ "$(findmnt -n -o SOURCE /)" == /dev/mmcblk0p2 ]]
[[ "$(readlink -f /run/current-system)" == "$expected" ]]
unit=korri-chromium-kiosk.service
view=/run/systemd/system/korri-chromium-kiosk.service.d/60-input-view.conf
[[ "$(systemctl show "$unit" -p DropInPaths --value)" == "$(cat "$stage/original-dropins")" ]]
[[ "$(systemctl show "$unit" -p ActiveState --value)" == active ]]
[[ "$(systemctl show "$unit" -p FreezerState --value)" == running ]]
[[ "$(systemctl show korri-inputd.service -p StatusText --value)" == Ready ]]
original=$(systemctl show "$unit" -p ExecStart --value | grep -oE '/nix/store/[a-z0-9]{32}-korri-chromium-kiosk/bin/korri-chromium-kiosk' | head -1)
jq=$(grep -oE '/nix/store/[a-z0-9]{32}-jq[^ /]*/bin/jq' "$original")
no_game() {
  local status
  status=$(runuser -u korri-inputd -g korri-control -- curl --silent --show-error --max-time 4 --unix-socket /run/korrid-control/control.sock -H 'Content-Type: application/json' --data '{"_tag":"app.session.status","payload":{}}' http://localhost/rpc) || return 1
  printf '%s' "$status" | "$jq" -e -f "$stage/no-game.jq" >/dev/null
}
no_game
portal_parent=$(cat "$stage/portal-parent")
[[ "$portal_parent" =~ ^/sys/devices/virtual/input/input[0-9]+$ ]]
[[ "$(cat "$portal_parent/phys")" == korri/inputd/portal ]]
# Compare against current kernel identities. A changed target set requires a new trial.
for node in /sys/class/input/js*; do
  parent=$(readlink -f "$node/device")
  [[ "$parent" =~ ^/sys/devices/virtual/input/input[0-9]+$ ]] || continue
  [[ "$parent" == "$portal_parent" ]] && continue
  printf 'TemporaryFileSystem=%s:ro,mode=755\n' "$parent"
done | sort -u >"$stage/current-view"
grep '^TemporaryFileSystem=/' "$stage/override" | sort -u >"$stage/tested-view"
cmp "$stage/current-view" "$stage/tested-view"
if grep -Ev '^TemporaryFileSystem=/sys/devices/virtual/input/input[0-9]+:ro,mode=755$' "$stage/tested-view"; then exit 1; fi
had_view=0
previous_digest=absent
if [[ -e "$view" ]]; then
  [[ ! -L "$view" && "$(stat -c '%u' "$view")" == 0 ]]
  grep -Fx '# Managed by rpminiv2-input-iterate' "$view" >/dev/null
  cp "$view" "$stage/previous-view"
  previous_digest=$(sha256sum "$view" | cut -d' ' -f1)
  had_view=1
fi
{
  printf '# Managed by rpminiv2-input-iterate\n[Service]\nTemporaryFileSystem=\n'
  cat "$stage/tested-view"
} >"$stage/working-view"
chmod 0644 "$stage/working-view"
new_digest=$(sha256sum "$stage/working-view" | cut -d' ' -f1)
changed=1
rollback() {
  trap '' HUP INT TERM
  [[ "$changed" == 1 ]] || return 0
  no_game || return 1
  local current_digest=absent
  if [[ -e "$view" ]]; then
    [[ ! -L "$view" ]] || return 1
    current_digest=$(sha256sum "$view" | cut -d' ' -f1) || return 1
  fi
  [[ "$current_digest" == "$new_digest" || "$current_digest" == "$previous_digest" ]] || return 1
  if [[ "$had_view" == 1 ]]; then
    install -m 0644 "$stage/previous-view" "$view" || return 1
  else
    rm -f "$view" || return 1
  fi
  systemctl daemon-reload || return 1
  systemctl restart "$unit" || return 1
  local actual_dropins
  actual_dropins=$(systemctl show "$unit" -p DropInPaths --value) || return 1
  [[ "$actual_dropins" == "$(cat "$stage/original-dropins")" ]] || return 1
  [[ "$(systemctl show "$unit" -p ActiveState --value)" == active ]] || return 1
  local pid
  pid=$(systemctl show "$unit" -p MainPID --value) || return 1
  [[ "$(readlink -f "/proc/$pid/exe")" == "$(cat "$stage/original-executable")" ]] || return 1
}
finish() {
  local code=$?
  trap - EXIT
  if ! rollback; then
    echo 'STOP: working-view rollback failed' >&2
    exit 90
  fi
  exit "$code"
}
trap finish EXIT
trap 'exit 130' HUP INT TERM
mv "$stage/working-view" "$view"
systemctl daemon-reload
systemctl restart "$unit"
[[ "$(systemctl show "$unit" -p DropInPaths --value)" == "$view" ]]
pid=$(systemctl show "$unit" -p MainPID --value)
[[ "$(readlink -f "/proc/$pid/exe")" == "$(cat "$stage/original-executable")" ]]
# Wake the display via the already configured native activity operation.
runuser -u korri -- /nix/store/vm7v8ak4ix5bni24l3cswz2iwqi9ya8k-sway-unwrapped-1.11/bin/swaymsg -s /run/korri-compositor/sway-ipc.sock 'seat * idle_notify' >/dev/null
changed=0
trap - EXIT HUP INT TERM
echo 'KEPT: production kiosk with passing per-boot input view; no diagnostic launcher or trace library'

#!/usr/bin/env bash
# Explicit installed bash/Python only. Never invoke nix-shell on the device.
set -euo pipefail
umask 077
mode=${1:?stage, run, restore or verify}
if [[ "$mode" == stage ]]; then
  [[ "$EUID" == 0 ]]
  [[ "$(tr -d '\0' </proc/device-tree/model)" == 'Retroid Pocket Mini V2' ]]
  [[ "$(findmnt -n -o SOURCE /)" == /dev/mmcblk0p2 ]]
  [[ "$(findmnt -n -o SOURCE /boot)" == /dev/mmcblk0p1 ]]
  [[ "$(cat /sys/class/block/mmcblk0/device/cid)" == 1d41445553440000200000365a019700 ]]
  [[ "$(findmnt -n -o FSTYPE /run)" == tmpfs ]]
  stage=$(mktemp -d /run/korri-input-trial.XXXXXXXX)
  chmod 0755 "$stage"
  install -d -m 0755 "$stage/bin" "$stage/assets" "$stage/inputs"
  install -d -m 0700 "$stage/baseline" "$stage/control" "$stage/evidence"
  printf '%s\n' "$stage"
  exit 0
fi
stage=${2:?stage}
python=${3:?absolute installed Python interpreter}
shift 3
[[ "$EUID" == 0 && "$stage" =~ ^/run/korri-input-trial\.[A-Za-z0-9]{8}$ ]]
[[ ! -L "$stage" && "$(stat -c '%u:%a' "$stage")" == 0:755 ]]
[[ "$python" == /nix/store/*/bin/python3 && -x "$python" ]]
[[ "$mode" == run || "$mode" == restore || "$mode" == verify ]]
# The outer shell owns the real plugin-host lock across BOTH the candidate and
# rollback. A separate restore/verify invocation must acquire it too.
[[ -f /var/lib/korri-plugin-host/lock && ! -L /var/lib/korri-plugin-host/lock ]]
[[ "$(stat -c '%u' /var/lib/korri-plugin-host/lock)" == 0 ]]
exec 9<>/var/lib/korri-plugin-host/lock
flock -n 9
if [[ "$mode" != run ]]; then
  timeout --signal=TERM --kill-after=10s 180s "$python" "$stage/run.py" "$mode" "$stage" "$@" 9>&-
  exit $?
fi
finish() {
  code=$?
  trap - EXIT
  trap '' HUP INT TERM
  if [[ -f "$stage/baseline/state.json" ]]; then
    if ! timeout --signal=TERM --kill-after=10s 180s "$python" "$stage/run.py" restore "$stage"; then
      echo 'STOP: rollback failed; baseline retained privately; do not start another trial' >&2
      exit 90
    fi
    if ! timeout --signal=TERM --kill-after=5s 45s "$python" "$stage/run.py" verify "$stage"; then
      echo 'STOP: independent rollback verification failed' >&2
      exit 91
    fi
  fi
  exit "$code"
}
trap finish EXIT
trap 'exit 130' HUP INT TERM
# Coreutils timeout supervises the entire process group, not only Python.
timeout --signal=TERM --kill-after=10s 180s "$python" "$stage/run.py" trial "$stage" "$@" 9>&-

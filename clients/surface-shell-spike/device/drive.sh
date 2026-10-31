#!/usr/bin/env nix-shell
#! nix-shell -i bash -p openssh coreutils gnutar gnugrep
# shellcheck shell=bash
# THROWAWAY workstation driver. No build commands execute on the device.
set -euo pipefail
if [[ $# -lt 1 ]]; then
  echo "usage: $0 BUNDLE.tar.gz [--surfaces zone,pico] [--hold-seconds 120] [--results DIRECTORY]" >&2; exit 2
fi
archive=$(realpath "$1")
shift
packages=zone,pico
hold_seconds=120
results=''
while [[ $# -gt 0 ]]; do
  [[ $# -ge 2 ]] || { echo "missing value for $1" >&2; exit 2; }
  case $1 in
    --surfaces) packages=$2 ;;
    --hold-seconds) hold_seconds=$2 ;;
    --results) results=$2 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift 2
done
if [[ ! $hold_seconds =~ ^[0-9]+$ || ${#hold_seconds} -gt 4 ]] || (( 10#$hold_seconds > 3600 )); then
  echo 'hold-seconds must be 0..3600' >&2; exit 2
fi
bundle=$(basename "$archive" .tar.gz)
[[ $bundle =~ ^surface-shell-[A-Za-z0-9._-]+$ ]] || { echo 'bundle name must start surface-shell-' >&2; exit 2; }
[[ $packages =~ ^[A-Za-z0-9_/-]+(,[A-Za-z0-9_/-]+)*$ ]] || { echo 'invalid package list' >&2; exit 2; }
# Fail locally before any SSH/SCP if the selected runtime packages are absent.
listing=$(tar tzf "$archive")
IFS=',' read -r -a selected <<< "$packages"
for package in "${selected[@]}"; do
  case "/$package/" in */../*|*/./*) echo 'invalid package path' >&2; exit 2 ;; esac
  for entry in main.slint logic.js; do
    grep -Fxq "$bundle/bin/surfaces/$package/$entry" <<< "$listing" || {
      echo "bundle is missing $package/$entry; rebuild it with SHELL_BUNDLE_ROOT pointing to the package checkout" >&2
      exit 2
    }
  done
done
here=$(cd "$(dirname "$0")" && pwd)
results=${results:-$here/../evidence/device-$(date +%Y%m%d-%H%M%S)}
[[ ! -e $results ]] || { echo 'results already exist' >&2; exit 2; }
SSH=(ssh -p 2222 -o BatchMode=yes -o ConnectTimeout=8)
SCP=(scp -P 2222 -o BatchMode=yes -o ConnectTimeout=8)
host=${SHELL_DEVICE:-root@10.42.5.1}
"${SCP[@]}" "$archive" "$here/surface-shell-run.sh" "$host:/tmp/"
"${SCP[@]}" "$here/measure.sh" "$host:/tmp/surface-shell-measure.sh"
remote_results="/tmp/surface-shell-results-$(date +%Y%m%d-%H%M%S)-$$"
status=0
"${SSH[@]}" "$host" sh /tmp/surface-shell-run.sh "$bundle" "$packages" "$remote_results" "$hold_seconds" || status=$?
mkdir -p "$results"
"${SCP[@]}" -r "$host:$remote_results/." "$results/"
# The evidence is local now. Remove only this spike's /tmp artefacts, never
# an active transient client. This includes the two earlier failed runs.
"${SSH[@]}" "$host" "if systemctl is-active -q surface-shell-spike.service; then echo 'cleanup blocked: shell still active' >&2; exit 1; fi; rm -rf -- /tmp/surface-shell-sp /tmp/surface-shell-sp.tar.gz /tmp/surface-shell-run.sh /tmp/surface-shell-measure.sh /tmp/surface-shell-results* '/tmp/$bundle' '/tmp/$bundle.tar.gz'"

echo "results: $results; device results: $remote_results; device status: $status"
exit "$status"

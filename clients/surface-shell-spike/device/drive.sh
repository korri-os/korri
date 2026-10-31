#!/usr/bin/env nix-shell
#! nix-shell -i bash -p openssh coreutils
# shellcheck shell=bash
# THROWAWAY workstation driver. No build commands execute on the device.
set -euo pipefail
if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: $0 BUNDLE.tar.gz PACKAGE_LIST [RESULTS_DIRECTORY]" >&2; exit 2
fi
archive=$(realpath "$1")
packages=$2
bundle=$(basename "$archive" .tar.gz)
[[ $bundle =~ ^surface-shell-[A-Za-z0-9._-]+$ ]] || { echo 'bundle name must start surface-shell-' >&2; exit 2; }
[[ $packages =~ ^[A-Za-z0-9_/-]+(,[A-Za-z0-9_/-]+)*$ ]] || { echo 'invalid package list' >&2; exit 2; }
here=$(cd "$(dirname "$0")" && pwd)
results=${3:-$here/../evidence/device-$(date +%Y%m%d-%H%M%S)}
[[ ! -e $results ]] || { echo 'results already exist' >&2; exit 2; }
SSH=(ssh -p 2222 -o BatchMode=yes -o ConnectTimeout=8)
SCP=(scp -P 2222 -o BatchMode=yes -o ConnectTimeout=8)
host=${SHELL_DEVICE:-root@10.42.5.1}
"${SCP[@]}" "$archive" "$here/surface-shell-run.sh" "$host:/tmp/"
status=0
"${SSH[@]}" "$host" sh /tmp/surface-shell-run.sh "$bundle" "$packages" || status=$?
mkdir -p "$results"
"${SCP[@]}" -r "$host:/tmp/surface-shell-results/." "$results/"
# Results remain on the device for diagnosis. Bundles are /tmp-only.
echo "results: $results; device status: $status"
exit "$status"

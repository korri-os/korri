#!/usr/bin/env nix-shell
#! nix-shell -i bash -p openssh coreutils
# Workstation driver: copy the native Zone bundle and scripts to the SP, run
# the device-side script, and copy the results back.
set -euo pipefail
HOST=root@10.42.5.1
SSH=(ssh -p 2222 -o BatchMode=yes -o ConnectTimeout=8)
SCP=(scp -P 2222 -o BatchMode=yes -o ConnectTimeout=8)
DEV=/home/simonwjackson/code/sandbox/korri/.worktrees/c-8d4e3919bff7b93e834d09610b1cc90a9ad0df583f408c598c106609c7588f0d/clients/zone-native-spike/device
HERE=$(cd "$(dirname "$0")" && pwd)
RESULTS=${1:-$HERE/results}

"${SCP[@]}" /tmp/zone-native-spike-sp.tar.gz "$HERE/zone-native-run.sh" "$HOST:/tmp/"
"${SCP[@]}" "$DEV/measure.sh" "$HOST:/tmp/zone-spike-measure.sh"
"${SSH[@]}" "$HOST" sh /tmp/zone-native-run.sh || echo "device script exited $?"
rm -rf "$RESULTS"
mkdir -p "$RESULTS"
"${SCP[@]}" -r "$HOST:/tmp/zone-native-results/." "$RESULTS/"
"${SSH[@]}" "$HOST" rm -rf /tmp/zone-native-spike-sp /tmp/zone-native-spike-sp.tar.gz /tmp/zone-native-run.sh /tmp/zone-spike-measure.sh /tmp/zone-native-results
ls -la "$RESULTS"

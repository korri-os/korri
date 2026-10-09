#!/usr/bin/env bash
# Run a command again when GitHub refuses a release download.
#
# Korri's Nix caches are GitHub releases. From a CI runner, GitHub sometimes
# answers a release download with 403. Nix treats 401, 403 and 407 as final
# (filetransfer.cc: "Don't retry on authentication/authorization failures"), so
# download-attempts does not help, and it reports the refusal as a file that
# "does not exist in binary cache". The refusals come in bursts, so this waits
# between whole attempts instead of retrying at once.
#
# narinfo-cache-negative-ttl = 0 stops Nix from remembering a refused lookup as
# a missing path, which would otherwise block the next attempt for an hour.
set -uo pipefail

attempts=3
pause=90
export NIX_CONFIG="${NIX_CONFIG:-}
narinfo-cache-negative-ttl = 0"

for attempt in $(seq 1 "$attempts"); do
  "$@" && exit 0
  status=$?
  if (( attempt == attempts )); then
    echo "failed after $attempts attempts" >&2
    exit "$status"
  fi
  echo "attempt $attempt failed with exit $status; retrying in ${pause}s" >&2
  sleep "$pause"
done

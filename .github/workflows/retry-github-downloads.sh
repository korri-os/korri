#!/usr/bin/env bash
# Run a command again when GitHub refuses or stalls release downloads.
#
# Korri's Nix caches are GitHub releases. From a CI runner, GitHub sometimes
# fails release downloads in one of two ways:
#
# - It answers 403. Nix treats 401, 403 and 407 as final (filetransfer.cc:
#   "Don't retry on authentication/authorization failures"), so
#   download-attempts does not help. Nix reports the refusal as a file that
#   "does not exist in binary cache". These refusals come in bursts, so this
#   script waits between whole attempts.
# - It sends nothing. Nix waits stalled-download-timeout (300 s by default)
#   before it retries a stalled request, and it tries 5 times. Seen in CI:
#   696 stalled requests filled a 60-minute job. A 30 s limit lets Nix's own
#   retries move on.
#
# narinfo-cache-negative-ttl = 0 stops Nix from remembering a failed lookup
# as a missing path, which would block the next attempt for an hour.
set -uo pipefail

attempts=3
pause=90
export NIX_CONFIG="${NIX_CONFIG:-}
narinfo-cache-negative-ttl = 0
stalled-download-timeout = 30
connect-timeout = 15"

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

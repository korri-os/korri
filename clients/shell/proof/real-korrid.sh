#!/usr/bin/env bash
# Start a real korrid in host mode with the surface wire, for the shell.
# Usage: real-korrid.sh KORRID_BIN_DIR RUNTIME_DIR PORT
# Writes RUNTIME_DIR/shell-credentials/KORRID_RPC_CAPABILITY and prints the
# environment the shell needs (never the capability itself). Modelled on
# services/inputd/nix/dev-app.nix. Runs until killed.
set -euo pipefail
bin=$1 root=$2 port=$3
origin=http://127.0.0.1:5173
mkdir -m 0700 -p "$root"/{state,signer-credentials,shell-credentials}
mkdir -p "$root/storage"
printf 'label = "shell-proof"\n' >"$root/host.toml"
signer_socket=$root/local-signer.sock
umask 077
head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' >"$root/shell-credentials/KORRID_RPC_CAPABILITY"
KORRID_PRIVATE_STATE_ROOT="$root/state" "$bin/korrid" identity status |
  jq -er '.devicePublicKey | select(test("^[0-9a-f]{64}$"))' >"$root/signer-credentials/expected-device-public-key"
chmod 0400 "$root/signer-credentials/expected-device-public-key"
# Setup has ended here, so the shell shows no setup and korrid never restarts
# with exit 75 for setup's end, which would stop this proof. The device has
# no owner: no device makes a person key at start (ADR 0082).
install -m 0600 /dev/null "$root/state/onboarding-finished"

children=()
cleanup() {
  for pid in "${children[@]}"; do kill -TERM "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

systemd-socket-activate --now \
  -E "CREDENTIALS_DIRECTORY=$root/signer-credentials" \
  -E "KORRI_LOCAL_SIGNER_PEER_UID=$(id -u)" \
  -E "KORRI_LOCAL_SIGNER_PEER_GID=$(id -g)" \
  -l "$signer_socket" "$bin/korri-local-signer" &
children+=("$!")
for _ in $(seq 1 200); do
  [[ -S "$signer_socket" ]] && break
  sleep 0.02
done

KORRID_MODE=host \
  KORRID_ADDRESS="127.0.0.1:$port" \
  KORRID_HOST_CONFIG="$root/host.toml" \
  KORRID_STORAGE_ROOT="$root/storage" \
  KORRID_PRIVATE_STATE_ROOT="$root/state" \
  KORRID_LOCAL_SIGNER_SOCKET="$signer_socket" \
  KORRID_SHELL_ORIGIN="$origin" \
  CREDENTIALS_DIRECTORY="$root/shell-credentials" \
  "$bin/korrid" &
children+=("$!")
echo "CREDENTIALS_DIRECTORY=$root/shell-credentials KORRID_ADDRESS=127.0.0.1:$port KORRID_SHELL_ORIGIN=$origin"
wait -n "${children[@]}"

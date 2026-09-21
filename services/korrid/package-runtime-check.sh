#!/usr/bin/env bash
set -Eeuo pipefail

binary="${1:?korrid binary is required}"
signer_binary="${2:?local signer binary is required}"
socket_activate="${3:?systemd-socket-activate is required}"
bash_bin="${4:?bash path is required}"
curl_bin="${5:?curl path is required}"
coreutils_bin="${6:?coreutils bin directory is required}"
socat_bin="${7:?socat binary is required}"
jq_bin="${8:?jq binary is required}"
export PATH="$coreutils_bin"

root="$(mktemp -d)"
pid=''
signer_pid=''
relay_pid=''
cleanup() {
  result=$?
  if [[ -n "$pid" ]]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  if [[ -n "$signer_pid" ]]; then
    kill "$signer_pid" 2>/dev/null || true
    wait "$signer_pid" 2>/dev/null || true
  fi
  if [[ -n "$relay_pid" ]]; then
    kill "$relay_pid" 2>/dev/null || true
    wait "$relay_pid" 2>/dev/null || true
  fi
  if [[ "$result" != 0 ]]; then
    printf 'packaged korrid check failed (exit %s); private root mode: ' "$result" >&2
    stat -c '%a' "$root/private" >&2 || true
    if [[ -f "$root/stderr" ]]; then
      printf 'packaged korrid stderr (last 4096 bytes):\n' >&2
      tail -c 4096 "$root/stderr" >&2 || true
    fi
    if [[ -f "$root/signer-stderr" ]]; then
      printf 'packaged local signer stderr (last 4096 bytes):\n' >&2
      tail -c 4096 "$root/signer-stderr" >&2 || true
    fi
  fi
  rm -rf "$root"
  return "$result"
}
trap cleanup EXIT

mkdir -p "$root/home" "$root/storage" "$root/sunshine"
"$socat_bin" TCP-LISTEN:43998,reuseaddr,fork \
  SYSTEM:"echo connected >>$root/relay-connections" \
  >"$root/relay-stdout" 2>"$root/relay-stderr" &
relay_pid=$!
sleep 0.1
kill -0 "$relay_pid"
# Identity storage requires exactly 0700, regardless of the builder's umask.
mkdir -m 0700 "$root/private" "$root/signer-private" "$root/signer-credentials"
mkdir -m 0750 "$root/signer-public"
KORRID_PRIVATE_STATE_ROOT="$root/private" "$binary" identity status \
  | "$jq_bin" -er 'select(._tag == "Unowned" or ._tag == "Owned" or ._tag == "Revoked") | .devicePublicKey | select(test("^[0-9a-f]{64}$"))' \
  > "$root/signer-credentials/expected-device-public-key"
chmod 0400 "$root/signer-credentials/expected-device-public-key"
"$socket_activate" --now \
  -E "CREDENTIALS_DIRECTORY=$root/signer-credentials" \
  -E "KORRI_LOCAL_SIGNER_PRIVATE_STATE_ROOT=$root/signer-private" \
  -E "KORRI_LOCAL_SIGNER_PEER_UID=$(id -u)" \
  -E "KORRI_LOCAL_SIGNER_PEER_GID=$(id -g)" \
  -E "KORRI_LOCAL_SIGNER_PUBLIC_KEY_FILE=$root/signer-public/person.pub" \
  -l "$root/signer.sock" "$signer_binary" \
  >"$root/signer-stdout" 2>"$root/signer-stderr" &
signer_pid=$!
for _ in $(seq 1 100); do
  [[ -f "$root/signer-public/person.pub" ]] && break
  kill -0 "$signer_pid" 2>/dev/null || break
  sleep 0.01
done
[[ -f "$root/signer-public/person.pub" ]]
printf '%s\n' \
  'label = "package-check"' \
  '[[games]]' \
  'id = "inputd-gate"' \
  'title = "Input gate"' \
  "command = [\"$coreutils_bin/sleep\", \"1\"]" \
  >"$root/host.toml"
printf '#!%s\n%s\n' "$bash_bin" \
  'case " $* " in *" list-units "*) exit 0 ;; *" show "*) printf "not-found\\n"; exit 0 ;; *) exit 1 ;; esac' \
  >"$root/systemctl"
printf '#!%s\nexit 1\n' "$bash_bin" >"$root/systemd-run"
chmod 0700 "$root/systemctl" "$root/systemd-run"

HOME="$root/home" \
KORRID_MODE=host \
KORRID_ADDRESS=127.0.0.1:43999 \
KORRID_HOST_CONFIG="$root/host.toml" \
KORRID_STORAGE_ROOT="$root/storage" \
KORRID_PRIVATE_STATE_ROOT="$root/private" \
KORRID_LOCAL_SIGNER_SOCKET="$root/signer.sock" \
KORRID_LOCAL_SIGNER_PUBLIC_KEY_FILE="$root/signer-public/person.pub" \
KORRID_RELAYS='["ws://127.0.0.1:43998"]' \
KORRID_SUNSHINE_PRIVATE_STATE_ROOT="$root/sunshine" \
KORRID_SYSTEMCTL="$root/systemctl" \
KORRID_SYSTEMD_RUN="$root/systemd-run" \
  "$binary" >"$root/stdout" 2>"$root/stderr" &
pid=$!

healthy=false
for _ in $(seq 1 100); do
  status="$("$curl_bin" --silent --output /dev/null --write-out '%{http_code}' \
    --connect-timeout 1 --max-time 2 \
    http://127.0.0.1:43999/peer-rpc \
    -H 'content-type: application/json' \
    -d '{"_tag":"app.catalog.snapshot","payload":{}}' || true)"
  if [[ "$status" == 400 ]]; then
    healthy=true
    break
  fi
  kill -0 "$pid" 2>/dev/null || break
  sleep 0.05
done
[[ "$healthy" == true ]] || { printf 'packaged korrid did not start\n' >&2; exit 1; }

plaintext_status="$("$curl_bin" --silent --output /dev/null --write-out '%{http_code}' \
  --connect-timeout 1 --max-time 2 \
  http://127.0.0.1:43999/rpc \
  -H 'content-type: application/json' \
  -d '{"_tag":"app.catalog.snapshot","payload":{}}')"
[[ "$plaintext_status" == 426 ]]
[[ "$(stat -c '%a' "$root/private")" == 700 ]]
[[ -d "$root/private/identity" ]]
[[ "$(stat -c '%a' "$root/private/identity")" == 700 ]]
[[ "$(stat -c '%a' "$root/private/identity/device.key")" == 600 ]]
[[ "$(stat -c '%a' "$root/private/identity/owner.event.json")" == 600 ]]
[[ "$(stat -c '%a' "$root/signer-private/identity")" == 700 ]]
[[ "$(stat -c '%a' "$root/signer-private/identity/person.key")" == 600 ]]
[[ ! -e "$root/private/identity/person.key" ]]
[[ ! -e "$root/signer-private/identity/device.key" ]]
[[ ! -e "$root/home/.local/state/korri/identity" ]]
[[ ! -e "$root/relay-connections" ]]

printf 'korrid package runtime check passed with a local-only automatic owner (peer-rpc: HTTP 400; rpc: HTTP 426; relay connections: 0)\n'

#!/usr/bin/env bash
set -Eeuo pipefail

binary="${1:?korrid binary is required}"
bash_bin="${2:?bash path is required}"
curl_bin="${3:?curl path is required}"
jq_bin="${4:?jq path is required}"
coreutils_bin="${5:?coreutils bin directory is required}"
export PATH="$coreutils_bin"

root="$(mktemp -d)"
pid=''
cleanup() {
  if [[ -n "$pid" ]]; then
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  fi
  rm -rf "$root"
}
trap cleanup EXIT

mkdir -p "$root/home" "$root/private" "$root/storage" "$root/sunshine"
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
  sleep 0.05
done
[[ "$healthy" == true ]] || { printf 'packaged korrid did not start\n' >&2; exit 1; }

plaintext_status="$("$curl_bin" --silent --output /dev/null --write-out '%{http_code}' \
  --connect-timeout 1 --max-time 2 \
  http://127.0.0.1:43999/rpc \
  -H 'content-type: application/json' \
  -d '{"_tag":"app.catalog.snapshot","payload":{}}')"
[[ "$plaintext_status" == 426 ]]
[[ -d "$root/private/identity" ]]
[[ "$(stat -c '%a' "$root/private/identity")" == 700 ]]
[[ "$(stat -c '%a' "$root/private/identity/device.key")" == 600 ]]
[[ ! -e "$root/home/.local/state/korri/identity" ]]

printf 'korrid package runtime check passed\n'

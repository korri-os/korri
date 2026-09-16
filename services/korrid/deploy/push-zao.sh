#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils curl git jq nix openssh
# shellcheck shell=bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
started="$(date +%s)"
relay_json="${KORRID_RELAYS_JSON:?KORRID_RELAYS_JSON must contain one to eight production relay URLs}"
relay_json="$(jq -ce '
  if type != "array" or length < 1 or length > 8 then
    error("expected one to eight relays")
  elif any(.[]; type != "string" or (test("^wss://[^\\s#]+$") | not)) then
    error("production relays must use wss://")
  elif (unique | length) != length then
    error("relay URLs must be unique")
  else . end
' <<<"$relay_json")"
package_paths=(
  flake.nix
  flake.lock
  services/korrid
  plugins/libretro
  plugins/retroarch/plugin.ts
)
untracked_package="$(git -C "$root" ls-files --others --exclude-standard -- "${package_paths[@]}")"
revision="$(git -C "$root" describe --always --dirty)"
flake_ref="path:$root"
if git -C "$root" diff --quiet -- "${package_paths[@]}" && \
  git -C "$root" diff --cached --quiet -- "${package_paths[@]}" && \
  [[ -z "$untracked_package" ]]; then
  revision="$(git -C "$root" rev-parse HEAD)"
  flake_ref="git+file://$root?rev=$revision"
elif [[ -n "$untracked_package" && "$revision" != *-dirty ]]; then
  revision="$revision-dirty"
fi
package="$(nix build "$flake_ref#korrid" --no-link --print-out-paths)"
ssh_options=(
  -o BatchMode=yes
  -o ConnectTimeout=5
  -o ServerAliveInterval=5
  -o ServerAliveCountMax=2
)
revision_file="$(mktemp)"
relay_environment="$(mktemp)"
status_file="$(mktemp)"
remote_tmp=""
install_started=false
install_succeeded=false
cleanup() {
  rm -f "$revision_file" "$relay_environment" "$status_file"
  if [[ -n "$remote_tmp" ]]; then
    if [[ "$install_started" == false || "$install_succeeded" == true ]]; then
      ssh "${ssh_options[@]}" zao rm -rf -- "$remote_tmp" || true
    else
      echo "Deployment failed; recovery files retained on zao at $remote_tmp" >&2
    fi
  fi
}
trap cleanup EXIT
printf '%s\n' "$revision" > "$revision_file"
printf "KORRID_RELAYS='%s'\n" "$relay_json" > "$relay_environment"

NIX_SSHOPTS="${ssh_options[*]}" nix copy --to ssh://zao "$package"
remote_tmp="$(ssh "${ssh_options[@]}" zao mktemp -d /tmp/korrid-deploy.XXXXXX)"
ssh "${ssh_options[@]}" zao mkdir -p "$remote_tmp/catalog"
scp "${ssh_options[@]}" \
  "$root/services/korrid/deploy/korrid.service" \
  "zao:$remote_tmp/korrid.service"
scp "${ssh_options[@]}" \
  "$root/services/korrid/deploy/host.zao.toml" \
  "zao:$remote_tmp/host.toml"
scp "${ssh_options[@]}" \
  "$root/services/korrid/deploy/device.zao.yaml" \
  "zao:$remote_tmp/device.yaml"
scp "${ssh_options[@]}" \
  "$root/services/korrid/deploy/games.zao.yaml" \
  "zao:$remote_tmp/catalog/games.yaml"
scp "${ssh_options[@]}" \
  "$root/services/korrid/deploy/releases.zao.yaml" \
  "zao:$remote_tmp/catalog/releases.yaml"
scp "${ssh_options[@]}" \
  "$root/services/korrid/deploy/zao-remote.sh" \
  "zao:$remote_tmp/zao-remote.sh"
scp "${ssh_options[@]}" "$revision_file" "zao:$remote_tmp/revision"
scp "${ssh_options[@]}" "$relay_environment" "zao:$remote_tmp/environment"
install_started=true
ssh "${ssh_options[@]}" zao "$remote_tmp/zao-remote.sh" install "$package" "$remote_tmp"
install_succeeded=true

zao_url="${ZAO_KORRID_URL:-http://zao:43117}"
for _ in $(seq 1 40); do
  if response="$(curl --fail --silent --connect-timeout 1 --max-time 2 \
    "$zao_url/rpc" \
    -H 'content-type: application/json' \
    -d '{"_tag":"app.catalog.snapshot","payload":{}}')" && \
    jq -e '._tag == "app.catalog.snapshot"
      and .outcome._tag == "Ok"
      and any(.outcome.payload.games[]; .id == "neverball" and .host == "zao")
      and any(.outcome.payload.games[]; .id == "01K4J6K8Y00000000000000002" and .title == "Wario Land 4" and .host == "zao")' \
      <<<"$response" >/dev/null; then
    ssh "${ssh_options[@]}" zao \
      "KORRID_PRIVATE_STATE_ROOT=\"\$HOME/.local/state/korrid/private\" \"\$HOME/.local/state/korrid/current/bin/korrid\" identity status" \
      >"$status_file"
    android_upstreams="$(
      ZAO_KORRID_URL="$zao_url" \
        "$root/services/korrid/deploy/render-upstreams-android.sh" "$status_file"
    )"
    elapsed="$(( $(date +%s) - started ))"
    echo "zao korrid deployed with Neverball and Wario Land 4 in ${elapsed}s ($revision)"
    echo "Android secure peer config: $android_upstreams"
    exit 0
  fi
  sleep 0.25
done

echo "zao korrid did not serve the expected Neverball and Wario Land 4 catalog" >&2
exit 1

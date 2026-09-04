#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
status_file="${1:?usage: render-upstreams-android.sh STATUS_JSON [OUTPUT]}"
output="${2:-$root/.tmp/upstreams.android.json}"
template="$root/services/korrid/deploy/upstreams.android.json"
zao_url="${ZAO_KORRID_URL:-http://zao:43117}"

device_public_key="$(jq -er '
  select(._tag == "Unowned" or ._tag == "Owned" or ._tag == "Revoked")
  | .devicePublicKey
  | select(test("^[0-9a-f]{64}$"))
' "$status_file")"

mkdir -p "$(dirname "$output")"
jq --arg device_public_key "$device_public_key" --arg zao_url "$zao_url" '
  def valid_origin:
    test("^https?://(\\[[0-9A-Fa-f:.]+\\]|[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?)(:[0-9]{1,5})?$")
    and (
      (try (capture(":(?<port>[0-9]+)$").port | tonumber) catch 1) as $port
      | $port >= 1 and $port <= 65535
    );

  if ($zao_url | valid_origin | not) then
    error("ZAO_KORRID_URL must be an HTTP(S) origin without credentials, a path, a query, or a fragment")
  elif ([.[] | select(.label == "zao" and .kind == "native")] | length) != 1 then
    error("template must contain exactly one native zao peer")
  else
    map(
      if .label == "zao" and .kind == "native" then
        if .devicePublicKey != "__ZAO_DEVICE_PUBLIC_KEY_FROM_IDENTITY_STATUS__" then
          error("zao devicePublicKey is not the deployment placeholder")
        else
          .baseUrl = $zao_url
          | .devicePublicKey = $device_public_key
        end
      else
        .
      end
    )
  end
' "$template" >"$output.next"
mv -f "$output.next" "$output"
printf '%s\n' "$output"

#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
renderer="$root/services/korrid/deploy/render-upstreams-android.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
key="7bb368b270acb72d81856b7b7010d919ec4882afe7c3aaa56b7b6839e46b47f6"
printf '{"_tag":"Owned","devicePublicKey":"%s"}\n' "$key" >"$tmp/status.json"

"$renderer" "$tmp/status.json" "$tmp/default.json" >/dev/null
jq -e --arg key "$key" '
  . == [{
    label: "zao",
    kind: "native",
    baseUrl: "http://zao:43117",
    devicePublicKey: $key,
    moonlightAddress: "zao:47989"
  }]
' "$tmp/default.json" >/dev/null

ZAO_KORRID_URL=http://zao:39217 \
  "$renderer" "$tmp/status.json" "$tmp/override.json" >/dev/null
jq -e --arg key "$key" '
  .[0].baseUrl == "http://zao:39217"
  and .[0].devicePublicKey == $key
  and .[0].moonlightAddress == "zao:47989"
' "$tmp/override.json" >/dev/null

for invalid in \
  'http://user@zao:39217' \
  'http://zao:39217/path' \
  'http://zao:39217?query=yes' \
  'http://zao:39217#fragment' \
  'http://zao:0' \
  'http://zao:65536' \
  'http://zao:abc'
do
  if ZAO_KORRID_URL="$invalid" \
    "$renderer" "$tmp/status.json" "$tmp/invalid.json" >"$tmp/invalid.stdout" 2>"$tmp/invalid.stderr"
  then
    echo "invalid ZAO_KORRID_URL passed: $invalid" >&2
    exit 1
  fi
done

printf 'Android upstream rendering contract passed\n'

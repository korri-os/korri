#!/usr/bin/env nix-shell
#! nix-shell -i bash -p jq ripgrep
# Prove that the WiFi SSID and key never enter the flake, and that the
# populated env file lands on the image root when KORRI_WIFI_ENV is set.
#
#   nix/base/wifi-check.sh            pure and impure evaluation checks
#   KORRI_WIFI_ENV=... nix/base/wifi-check.sh
set -euo pipefail
cd "$(dirname "$0")/../.."

fail() { echo "FAIL: $*" >&2; exit 1; }

echo "== tracked files must carry no SSID or PSK literal"
if git grep -n -E 'psk = "[^$]|ssid = "[^$]' -- '*.nix'; then
  fail "a literal ssid or psk is committed"
fi

for host in rg353m odin2portal; do
  echo "== $host: pure evaluation (no env file)"
  profiles="$(nix eval --json ".#nixosConfigurations.$host.config.networking.networkmanager.ensureProfiles" 2>/dev/null)"
  ssid="$(jq -r '.profiles.korri.wifi.ssid' <<<"$profiles")"
  psk="$(jq -r '.profiles.korri."wifi-security".psk' <<<"$profiles")"
  envfiles="$(jq -c '.environmentFiles' <<<"$profiles")"
  [ "$ssid" = '$WIFI_SSID' ] || fail "$host ssid is '$ssid'"
  [ "$psk" = '$WIFI_PSK' ] || fail "$host psk is '$psk'"
  [ "$envfiles" = '["/etc/korri/wifi.env"]' ] || fail "$host environmentFiles is $envfiles"
  populate="$(nix eval --raw ".#nixosConfigurations.$host.config.sdImage.populateRootCommands")"
  if rg -q 'wifi.env' <<<"$populate"; then
    fail "$host pure build would still try to install wifi.env"
  fi
  echo "   ok: placeholders only, no env file in pure build"
done

if [ -n "${KORRI_WIFI_ENV:-}" ]; then
  echo "== impure evaluation with KORRI_WIFI_ENV=$KORRI_WIFI_ENV"
  for host in rg353m odin2portal; do
    populate="$(nix eval --impure --raw ".#nixosConfigurations.$host.config.sdImage.populateRootCommands")"
    rg -q 'install -m 0600 /nix/store/[a-z0-9]+-[^ ]+ \./files/etc/korri/wifi.env' <<<"$populate" \
      || fail "$host impure build does not install wifi.env: $populate"
    if rg -q -F "$(sed -n 's/^WIFI_PSK=//p' "$KORRI_WIFI_ENV")" <<<"$populate"; then
      fail "$host PSK appears in the populate command itself"
    fi
    echo "   ok: $host installs wifi.env from the build host"
  done
fi

echo "all WiFi secret checks passed"

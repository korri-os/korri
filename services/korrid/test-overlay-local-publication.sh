#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils gnugrep gnused
# shellcheck shell=bash
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
# shellcheck source=../../clients/android/local-launch-publication.sh disable=SC1091
source "$ROOT/clients/android/local-launch-publication.sh"

launch_id=0123456789abcdef0123456789abcdef
valid="launchId=$launch_id event=published gameId=01K4J6K8Y00000000000000002 package=com.korri.retroarch launcher=retroarch"
[[ "$(korri_parse_wario_retroarch_publication "$valid")" == "$launch_id" ]]

assert_rejected() {
  local label="$1"
  local fixture="$2"
  if korri_parse_wario_retroarch_publication "$fixture" >/dev/null 2>&1; then
    echo "publication parser accepted $label fixture" >&2
    exit 1
  fi
}

assert_rejected empty ''
assert_rejected multiple "$valid
$valid"
assert_rejected malformed-launch-id \
  'launchId=not-a-launch event=published gameId=01K4J6K8Y00000000000000002 package=com.korri.retroarch launcher=retroarch'
assert_rejected wrong-game \
  "launchId=$launch_id event=published gameId=other package=com.korri.retroarch launcher=retroarch"
assert_rejected wrong-package \
  "launchId=$launch_id event=published gameId=01K4J6K8Y00000000000000002 package=com.retroarch.aarch64 launcher=retroarch"
assert_rejected wrong-launcher \
  "launchId=$launch_id event=published gameId=01K4J6K8Y00000000000000002 package=com.korri.retroarch launcher=other"
assert_rejected control-token-leak \
  "$valid controlToken=secret"
assert_rejected control-port-leak \
  "$valid controlPort=49152"
assert_rejected capability-leak \
  "$valid capability=secret"
assert_rejected authorization-leak \
  "$valid authorization:Bearer-secret"

printf 'Overlay local publication fixtures passed\n'

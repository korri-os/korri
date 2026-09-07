#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils git nix
# shellcheck shell=bash
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
REVIEW_ROOT="${1:-}"

if [[ "${KORRI_PLUGIN_ROUTE_REVIEW_IN_SHELL:-}" != "1" ]]; then
  export KORRI_ROOT="$ROOT"
  export KORRI_PLUGIN_ROUTE_REVIEW_IN_SHELL=1
  exec nix develop "$ROOT#korrid" --command "$0" "$@"
fi

if [[ -z "$REVIEW_ROOT" ]]; then
  REVIEW_ROOT="$(mktemp -d)"
  trap 'rm -rf "$REVIEW_ROOT"' EXIT
  mkdir -p "$REVIEW_ROOT/catalog"
  cp "$ROOT/docs/research/android-app-plugin-schema-checkpoint/device.yaml" \
    "$REVIEW_ROOT/device.yaml"
  cp "$ROOT/docs/research/android-app-plugin-schema-checkpoint/catalog/games.yaml" \
    "$REVIEW_ROOT/catalog/games.yaml"
  cp "$ROOT/docs/research/android-app-plugin-schema-checkpoint/catalog/releases.yaml" \
    "$REVIEW_ROOT/catalog/releases.yaml"
fi

cargo run --quiet \
  --manifest-path "$ROOT/services/korrid/Cargo.toml" \
  --bin plugin_route_probe -- "$REVIEW_ROOT" --review

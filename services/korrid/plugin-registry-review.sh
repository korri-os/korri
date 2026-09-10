#!/usr/bin/env bash
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
case $# in
  0) NAMESPACE='@korri'; PLUGIN="$ROOT/services/korrid/plugins/android-app.plugin.ts" ;;
  2) NAMESPACE="$1"; PLUGIN="$2" ;;
  *) echo 'usage: plugin-registry-review.sh [PUBLISHER_NAMESPACE plugin.ts]' >&2; exit 2 ;;
esac

if [[ "${KORRI_PLUGIN_REVIEW_IN_SHELL:-}" != "1" ]]; then
  export KORRI_ROOT="$ROOT"
  export KORRI_PLUGIN_REVIEW_IN_SHELL=1
  exec nix develop "$ROOT#korrid" --command "$0" "$@"
fi

exec cargo run --quiet \
  --manifest-path "$ROOT/services/korrid/Cargo.toml" \
  --bin plugin_registry_probe -- "$NAMESPACE" "$PLUGIN" --review

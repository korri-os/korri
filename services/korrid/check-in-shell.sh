#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash curl coreutils gcc
# shellcheck shell=bash
# THROWAWAY PROTOTYPE implementation; invoked by run-spike.sh in its devshell.
set -euo pipefail

# Child Nix-shebang helpers return JSON. Do not replay this development
# shell's startup banner inside their captured stdout.
unset shellHook

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
CRATE="$ROOT/services/korrid"
GENERATED_TS="$ROOT/contracts/generated/korrid.ts"

cd "$CRATE"
# Focused treaty regeneration uses the same Typeshare invocation as the full gate.
if [[ "${1:-}" == "--types-only" && $# == 1 ]]; then
  typeshare . --lang=typescript --output-file="$GENERATED_TS"
  sed -i -e 's/[[:space:]]\+$//' -e '${/^$/d;}' "$GENERATED_TS"
  exit 0
fi
cargo fmt --check
cargo check
KORRI_SCRIPT_FIXTURES_IN_SHELL=1 bash "$CRATE/script-fixtures-setup.sh"
cargo test
KORRI_CONFIG_REVIEW_IN_SHELL=1 "$CRATE/config-snapshot-review.sh"
"$CRATE/deploy/test-zao-remote.sh"
typeshare . --lang=typescript --output-file="$GENERATED_TS"
# Typeshare 1.13 emits trailing spaces and an extra final blank line.
sed -i -e 's/[[:space:]]\+$//' -e '${/^$/d;}' "$GENERATED_TS"

cd "$ROOT/clients/portal"
bun install --frozen-lockfile --ignore-scripts
# Both registered surfaces compile from source and must share the portal's
# React instance. A missing local dependency must not fall through to a parent
# checkout when this check runs in a worktree.
for surface in shift pico; do
  cd "$ROOT/surfaces/$surface"
  bun install --frozen-lockfile --ignore-scripts
  for package in react react-dom; do
    rm -rf "$ROOT/surfaces/$surface/node_modules/$package"
    ln -s "$ROOT/clients/portal/node_modules/$package" \
      "$ROOT/surfaces/$surface/node_modules/$package"
  done
done

cd "$ROOT/clients/portal"
bun run typecheck
bun test

# Shift is its own package with its own toolchain: check it on its own terms so
# a surface break is reported as a surface break, not as a portal failure.
cd "$ROOT/surfaces/shift"
bun run typecheck
bun test

cd "$CRATE"
cargo build --release --bin korrid
export KORRID_MODE="brain"
export KORRID_RPC_CAPABILITY="check-capability"
export KORRID_ADDRESS="127.0.0.1:49117"
export KORRID_SPIKE_URL="http://$KORRID_ADDRESS"
local_storage_root="$(mktemp -d)"
mkdir -p "$local_storage_root/catalog"
# The reviewed checkpoint gives brain mode a real catalog to serve. It resolves
# no route here, and it must not pretend to: standalone korrid takes its runners
# from /run/korri-plugin-host/enabled-packages.json, which only the plugin host
# writes as root. The listing proof lives in that host's VM test, where a plugin
# is installed and enabled the way a device installs one.
cp "$ROOT/docs/research/retroarch-plugin-route/device.yaml" "$local_storage_root/device.yaml"
cp "$ROOT/docs/research/retroarch-plugin-route/catalog/games.yaml" "$local_storage_root/catalog/games.yaml"
cp "$ROOT/docs/research/retroarch-plugin-route/catalog/releases.yaml" "$local_storage_root/catalog/releases.yaml"
export KORRI_LOCAL_STORAGE_ROOT="$local_storage_root"
export KORRID_PRIVATE_STATE_ROOT="$local_storage_root/private"
if (exec 9<>/dev/tcp/127.0.0.1/49117) 2>/dev/null; then
  echo 'korrid check port 49117 is already occupied' >&2
  exit 1
fi
"$CARGO_TARGET_DIR/release/korrid" &
server_pid=$!
cleanup_server() {
  kill "$server_pid" 2>/dev/null || true
  rm -rf "$local_storage_root"
}
trap cleanup_server EXIT
server_ready=false
for _ in $(seq 1 20); do
  if ! kill -0 "$server_pid" 2>/dev/null; then
    wait "$server_pid" || true
    echo 'fresh korrid check server exited before becoming ready' >&2
    exit 1
  fi
  if curl --fail --silent "$KORRID_SPIKE_URL/rpc" \
      -H 'content-type: application/json' \
      -H "authorization: Bearer $KORRID_RPC_CAPABILITY" \
      -d '{"_tag":"system.health","payload":{}}' >/dev/null; then
    sleep 0.05
    if ! kill -0 "$server_pid" 2>/dev/null; then
      wait "$server_pid" || true
      echo 'fresh korrid check server exited after readiness probe' >&2
      exit 1
    fi
    server_ready=true
    break
  fi
  sleep 0.25
done
if [[ "$server_ready" != true ]]; then
  echo 'fresh korrid check server did not become ready' >&2
  exit 1
fi

# The portal's own client speaks to that server: health, then the catalog.
cd "$ROOT/clients/portal"
bun src/korrid/smoke.ts

# The installed proof is the whole app, not only its hidden RPC.
cd "$ROOT"
if [[ -n "${KORRI_PORTAL_BUNDLE:-}" ]]; then
  "$KORRI_PORTAL_BUNDLE"
else
  nix run "$ROOT#portal-bundle"
fi

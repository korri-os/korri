#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash bun git
# One React instance for standalone portal checks and the full host gate.
set -euo pipefail
ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
cd "$ROOT/clients/portal"
bun install --frozen-lockfile --ignore-scripts
for surface in pico boxbuster zone; do
  cd "$ROOT/surfaces/$surface"
  bun install --frozen-lockfile --ignore-scripts
  for package in react react-dom; do
    rm -rf "$ROOT/surfaces/$surface/node_modules/$package"
    ln -s "$ROOT/clients/portal/node_modules/$package" \
      "$ROOT/surfaces/$surface/node_modules/$package"
  done
done

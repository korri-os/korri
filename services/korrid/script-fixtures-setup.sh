#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash git
# shellcheck shell=bash
# Build-machine source fixtures for script_packages; never an evaluator loader.
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
if [[ "${KORRI_SCRIPT_FIXTURES_IN_SHELL:-}" != "1" ]]; then
  export KORRI_ROOT="$ROOT"
  export KORRI_SCRIPT_FIXTURES_IN_SHELL=1
  exec nix develop "$ROOT#korrid" --command \
    bash "$ROOT/services/korrid/script-fixtures-setup.sh"
fi

# These two core-owned locks own the native source versions used by the public
# snapshot tests. Always check the locks; a warm node_modules is not evidence.
for package in \
  services/korrid/tests/fixtures/script-packages/retroarch \
  docs/research/retroarch-effect-quickjs-probe; do
  (
    cd "$ROOT/$package"
    bun install --frozen-lockfile --ignore-scripts
  )
done

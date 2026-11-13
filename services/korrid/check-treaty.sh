#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash git typeshare gnused diffutils
# Compare generated output without repairing a stale checked-in treaty.
set -euo pipefail
ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
mode="${1:---check}"
if [[ $# -gt 1 || ( "$mode" != --check && "$mode" != --write ) ]]; then
  echo 'usage: check-treaty.sh [--check | --write]' >&2
  exit 2
fi
# One generated file per Rust treaty.
generate() {
  local crate="$1" output="$2" generated
  generated="$(mktemp)"
  (
    cd "$crate"
    typeshare . --lang=typescript --config-file=typeshare.toml --output-file="$generated"
  )
  # Typeshare 1.13 emits trailing spaces and an extra final blank line.
  sed -i -e 's/[[:space:]]\+$//' -e '${/^$/d;}' "$generated"
  if [[ "$mode" == --write ]]; then
    cp "$generated" "$output"
  elif ! diff -u "$output" "$generated"; then
    rm -f "$generated"
    echo 'Treaty drift: regenerate with nix run .#korrid-check -- --types-only' >&2
    exit 1
  fi
  rm -f "$generated"
}
generate "$ROOT/services/korrid" "$ROOT/contracts/generated/korrid.ts"
generate "$ROOT/contracts/settings" "$ROOT/contracts/generated/settings.ts"
generate "$ROOT/contracts/surface" "$ROOT/contracts/generated/surface.ts"
generate "$ROOT/contracts/viewer" "$ROOT/contracts/generated/viewer.ts"

#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash git typeshare gnused diffutils
# Compare generated output without repairing a stale checked-in treaty.
set -euo pipefail
ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
output="$ROOT/contracts/generated/korrid.ts"
mode="${1:---check}"
if [[ $# -gt 1 || ( "$mode" != --check && "$mode" != --write ) ]]; then
  echo 'usage: check-treaty.sh [--check | --write]' >&2
  exit 2
fi
generated="$(mktemp)"
trap 'rm -f "$generated"' EXIT
cd "$ROOT/services/korrid"
typeshare . --lang=typescript --output-file="$generated"
# Typeshare 1.13 emits trailing spaces and an extra final blank line.
sed -i -e 's/[[:space:]]\+$//' -e '${/^$/d;}' "$generated"
if [[ "$mode" == --write ]]; then
  cp "$generated" "$output"
else
  if ! diff -u "$output" "$generated"; then
    echo 'Treaty drift: regenerate with nix run .#korrid-check -- --types-only' >&2
    exit 1
  fi
fi

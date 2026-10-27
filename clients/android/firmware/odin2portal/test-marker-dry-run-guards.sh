#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DRY_RUN="$HERE/marker-dry-run.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir "$TMP/source"

if "$DRY_RUN" "$TMP/source" "$TMP/source/output" >"$TMP/inside.stdout" 2>"$TMP/inside.stderr"; then
  echo 'marker dry run accepted output inside source' >&2
  exit 1
fi
grep -F 'output must be outside the source directory' "$TMP/inside.stderr" >/dev/null

mkdir "$TMP/existing-output"
if "$DRY_RUN" "$TMP/source" "$TMP/existing-output" >"$TMP/existing.stdout" 2>"$TMP/existing.stderr"; then
  echo 'marker dry run accepted existing output' >&2
  exit 1
fi
grep -F 'output already exists' "$TMP/existing.stderr" >/dev/null

if "$DRY_RUN" "$TMP/source" "$TMP/checksum-output" >"$TMP/checksum.stdout" 2>"$TMP/checksum.stderr"; then
  echo 'marker dry run accepted a source without the stock checksums' >&2
  exit 1
fi
[[ ! -e "$TMP/checksum-output" ]]
grep -F 'source checksum verification failed' "$TMP/checksum.stderr" >/dev/null

printf 'odin2portal marker guard tests passed\n'

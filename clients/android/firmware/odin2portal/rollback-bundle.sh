#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$HERE/rollback-bundle-core.sh" \
  "${1:?usage: odin2portal-rollback-bundle <stock-source-directory> <output-directory>}" \
  "${2:?usage: odin2portal-rollback-bundle <stock-source-directory> <output-directory>}" \
  "$HERE/contract" \
  "$HERE/ROLLBACK.md"

#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$HERE/repack-core.sh" "${1:?usage: odin2portal-stock-repack <stock-source-directory> <output-directory>}" "${2:?usage: odin2portal-stock-repack <stock-source-directory> <output-directory>}" "$HERE/contract"

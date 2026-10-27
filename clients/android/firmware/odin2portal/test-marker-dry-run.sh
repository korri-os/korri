#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
"$HERE/test-append-build-prop-marker.sh"
"$HERE/test-marker-dry-run-guards.sh"

if [[ -z "${ODIN2PORTAL_STOCK_SOURCE:-}" ]]; then
  printf 'odin2portal marker integration skipped: set ODIN2PORTAL_STOCK_SOURCE to the private stock capture\n'
  exit 0
fi

SOURCE="$(cd "$ODIN2PORTAL_STOCK_SOURCE" && pwd -P)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir "$TMP/device-tools"
cp "$HERE/test/fixtures/bin/adb" "$HERE/test/fixtures/bin/fastboot" "$TMP/device-tools/"
chmod +x "$TMP/device-tools/adb" "$TMP/device-tools/fastboot"
export FAKE_TOOL_LOG="$TMP/device-tools.log"
: > "$FAKE_TOOL_LOG"
export PATH="$TMP/device-tools:$PATH"

"$HERE/marker-dry-run.sh" "$SOURCE" "$TMP/output"
grep -Fx 'ODIN2PORTAL_MARKER_DRY_RUN_VERIFIED' "$TMP/output/RESULT.txt" >/dev/null
grep -Fx 'flash ready: no' "$TMP/output/RESULT.txt" >/dev/null
[[ -f "$TMP/output/NON_FLASHABLE_ARTIFACTS/super.img.not-flashable" ]]
[[ -f "$TMP/output/NON_FLASHABLE_ARTIFACTS/product_a.img.not-flashable" ]]
[[ -f "$TMP/output/NON_FLASHABLE_ARTIFACTS/vbmeta_system_a.img.not-flashable" ]]
(
  cd "$TMP/output"
  sha256sum --check evidence/output-SHA256SUMS
)
[[ ! -s "$FAKE_TOOL_LOG" ]]
printf 'odin2portal marker integration passed\n'

#!/usr/bin/env bash
set -euo pipefail

ROOT="${KORRI_ROOT:-$(git rev-parse --show-toplevel)}"
COMMON="$ROOT/clients/android/app/src/main/jni/moonlight-core/moonlight-common-c"
TEST="$ROOT/clients/android/app/src/test-native/sunshine-runtime-settings-test.c"
OUT="$(mktemp "${TMPDIR:-/tmp}/korri-sunshine-runtime-settings-test.XXXXXX")"
trap 'rm -f "$OUT"' EXIT

"${KORRI_HOST_CC:-cc}" -std=c11 -Wall -Wextra -Werror -pedantic \
  -I"$COMMON/src" \
  "$COMMON/src/SunshineRuntimeSettings.c" \
  "$COMMON/src/SunshineRuntimeSettingsDispatch.c" \
  "$TEST" \
  -pthread \
  -o "$OUT"
"$OUT"

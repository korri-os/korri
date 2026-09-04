#!/usr/bin/env nix-shell
#! nix-shell -i bash -p bash coreutils curl gnugrep gnused android-tools unzip jq websocat
# shellcheck shell=bash
# Install the built APK and call Rust Axum over adb forward.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
APK="${KORRI_ANDROID_APK:-$ROOT/clients/android/app/build/outputs/apk/debug/app-arm64-v8a-debug.apk}"
PACKAGE="com.simonwjackson.korri.debug"
HOST_PORT=43118
ANDROID_STORAGE_ALIAS="/sdcard/korri"
ANDROID_STORAGE_ROOT="$ANDROID_STORAGE_ALIAS"
CHECKPOINT_CONFIG="$ROOT/docs/research/retroarch-plugin-route/config.yaml"
CHECKPOINT_LIBRARY="${KORRI_ANDROID_APP_ROUTE_CHECKPOINT_LIBRARY:-$ROOT/docs/research/retroarch-plugin-route/library.yaml}"
ANDROID_APP_PACKAGE="${KORRI_ANDROID_APP_PACKAGE:-com.playdigious.tmnt}"
EXPECT_RETROARCH_ROUTE="${KORRI_EXPECT_RETROARCH_ROUTE:-false}"
UPSTREAMS_CONFIG="${KORRI_ANDROID_UPSTREAMS_CONFIG:-$ROOT/.tmp/upstreams.android.json}"
CURL=(curl --connect-timeout 2 --max-time 5 --retry 2 --retry-connrefused)
DEBUG_CAPABILITY_SH="${KORRI_ANDROID_DEBUG_CAPABILITY_SH:-$ROOT/services/korrid/android-debug-capability.sh}"

require_wl4_local_launch_response() {
  local response="$1"

  if jq -e --arg storage_root "$ANDROID_STORAGE_ROOT" '
    def exact_keys($expected): type == "object" and (keys == ($expected | sort));

    exact_keys(["_tag", "outcome"])
    and ._tag == "app.local-games.launch"
    and (.outcome | exact_keys(["_tag", "payload"]))
    and (
      (
        .outcome._tag == "Ok"
        and (.outcome.payload | exact_keys(["component", "context", "directories", "disposition", "extras", "files", "integrity", "launchId", "launcherId"]))
        and (.outcome.payload.launchId | test("^[0-9a-f]{32}$"))
        and .outcome.payload.launcherId == "retroarch"
        and .outcome.payload.disposition == "fresh"
        and .outcome.payload.context.gameId == "wl4"
        and .outcome.payload.context.executor == {id: "retroarch-control", available: true}
        and (.outcome.payload.component | exact_keys(["className", "packageName"]))
        and .outcome.payload.component.packageName == "com.korri.retroarch"
        and .outcome.payload.component.className == "com.retroarch.browser.retroactivity.RetroActivityFuture"
        and (.outcome.payload.extras | exact_keys(["CONFIGFILE", "LIBRETRO", "ROM"]))
        and .outcome.payload.extras.ROM == ($storage_root + "/roms/wl4.gba")
        and .outcome.payload.extras.LIBRETRO == "/data/data/com.korri.retroarch/cores/mgba_libretro_android.so"
        and .outcome.payload.extras.CONFIGFILE == ($storage_root + "/retroarch.cfg")
        and .outcome.payload.directories == (["system", "saves", "states", "screenshots"] | map($storage_root + "/" + .))
        and (.outcome.payload.files | type == "array" and length == 1)
        and (.outcome.payload.files[0] | exact_keys(["content", "path"]))
        and .outcome.payload.files[0].path == ($storage_root + "/retroarch.cfg")
        and (.outcome.payload.files[0].content | contains("video_driver = \"gl\""))
        and (.outcome.payload.files[0].content | contains("kiosk_mode_enable = \"true\""))
        and (.outcome.payload.files[0].content | contains("network_cmd_enable = \"true\""))
        and (.outcome.payload.files[0].content | contains("network_cmd_port = \""))
        and (.outcome.payload.integrity | test("^[0-9a-f]{64}$"))
      )
      or (
        .outcome._tag == "Err"
        and (.outcome.payload | exact_keys(["code", "message"]))
        and .outcome.payload.code == "LocalRomMissing"
        and .outcome.payload.message == ("local ROM is missing: " + $storage_root + "/roms/wl4.gba")
      )
    )
  ' <<<"$response" >/dev/null; then
    return 0
  fi

  echo "WL4 launch probe returned neither a signed deferred RetroArch instruction nor the stable LocalRomMissing error: $response" >&2
  return 1
}

require_android_app_launch_response() {
  local response="$1"
  local android_app_package="${KORRI_ANDROID_APP_PACKAGE:-$ANDROID_APP_PACKAGE}"

  if jq -e --arg android_app_package "$android_app_package" '
    .outcome._tag == "Ok"
    and .outcome.payload.launcherId == "android-app"
    and .outcome.payload.component.packageName == $android_app_package
    and .outcome.payload.component.className == ""
    and .outcome.payload.extras == {}
    and .outcome.payload.directories == []
    and .outcome.payload.files == []
    and (.outcome.payload.integrity | type == "string" and length > 0)
  ' <<<"$response" >/dev/null; then
    return 0
  fi

  echo "Configured Android route did not return the signed android-app shape for package $android_app_package: $response" >&2
  return 1
}

resolve_android_storage_root() {
  local device="$1"
  local storage_alias="$2"
  local resolved_root

  adb -s "$device" shell "mkdir -p '$storage_alias'"
  resolved_root="$(adb -s "$device" shell "cd '$storage_alias' && pwd -P" | tr -d '\r' | tail -n 1)"
  if [[ "$resolved_root" != /* ]]; then
    echo "Android storage root did not resolve to an absolute path: $resolved_root" >&2
    return 1
  fi
  ANDROID_STORAGE_ROOT="$resolved_root"
}

if [[ "${KORRI_ANDROID_SMOKE_LIBRARY:-false}" == true && "${BASH_SOURCE[0]}" != "$0" ]]; then
  return 0
fi

EXPECT_INSTALLED_ROUTE=false
if [[ "${1:-}" == "--expect-installed-route" ]]; then
  EXPECT_INSTALLED_ROUTE=true
  shift
fi
DEVICE="${1:-${KORRI_ANDROID_DEVICE:-}}"
if [[ -z "$DEVICE" ]]; then
  echo "usage: android-smoke.sh [--expect-installed-route] <adb-serial>" >&2
  exit 1
fi
ADB_BIN="${KORRI_ADB_BIN:-$(command -v adb)}"
adb() {
  if ! timeout 15 "$ADB_BIN" "$@"; then
    echo "adb command failed or timed out: $*" >&2
    return 1
  fi
}

if [[ "$DEVICE" == *:* ]]; then
  timeout 15 "$ADB_BIN" connect "$DEVICE" >/dev/null || true
fi
if ! timeout 15 "$ADB_BIN" -s "$DEVICE" wait-for-device; then
  echo "Android target is not reachable: $DEVICE" >&2
  exit 1
fi
resolve_android_storage_root "$DEVICE" "$ANDROID_STORAGE_ALIAS"

# grep must drain the whole listing: with pipefail, `grep -q` exiting at
# the first match SIGPIPEs unzip and fails the pipeline spuriously.
if ! unzip -l "$APK" | grep 'assets/portal/index.html' >/dev/null; then
  echo "APK is missing assets/portal/index.html" >&2
  exit 1
fi

if [[ ! -f "$UPSTREAMS_CONFIG" ]]; then
  echo "secure Android upstream config is missing: $UPSTREAMS_CONFIG" >&2
  echo "deploy Zao with services/korrid/deploy/push-zao.sh or set KORRI_ANDROID_UPSTREAMS_CONFIG" >&2
  exit 1
fi
adb -s "$DEVICE" push "$UPSTREAMS_CONFIG" "$ANDROID_STORAGE_ROOT/upstreams.json" >/dev/null
if [[ "$EXPECT_INSTALLED_ROUTE" == true ]]; then
  if ! adb -s "$DEVICE" exec-out cat "$ANDROID_STORAGE_ROOT/config.yaml" | cmp -s "$CHECKPOINT_CONFIG" -; then
    echo "Device config.yaml does not match the reviewed checkpoint bytes" >&2
    exit 1
  fi
  if ! adb -s "$DEVICE" exec-out cat "$ANDROID_STORAGE_ROOT/library.yaml" | cmp -s "$CHECKPOINT_LIBRARY" -; then
    echo "Device library.yaml does not match the reviewed checkpoint bytes" >&2
    exit 1
  fi
fi
if ! timeout 60 "$ADB_BIN" -s "$DEVICE" install -r "$APK"; then
  echo 'Android app install failed or timed out after 60s' >&2
  exit 1
fi
adb -s "$DEVICE" logcat -c
adb -s "$DEVICE" shell am start -S -n "$PACKAGE/com.limelight.KorriShellActivity" >/dev/null

port=""
portal_ready=""
for _ in $(seq 1 20); do
  line="$(adb -s "$DEVICE" logcat -d -s KorridServer:I 2>/dev/null | grep 'listening on 127.0.0.1:' | tail -1 || true)"
  port="$(printf '%s' "$line" | sed -n 's/.*127\.0\.0\.1:\([0-9][0-9]*\).*/\1/p')"
  portal_ready="$(adb -s "$DEVICE" logcat -d -s KorriPortal:I 2>/dev/null | grep 'title="Korri"' | tail -1 || true)"
  if [[ -n "$port" && -n "$portal_ready" ]]; then
    break
  fi
  sleep 1
done

if [[ -z "$port" || -z "$portal_ready" ]]; then
  echo "Complete app smoke failed (Rust port='$port', portal='$portal_ready')" >&2
  adb -s "$DEVICE" logcat -d -t 300 >&2
  exit 1
fi

# Readiness is established without a bearer. Deeper debug acceptance obtains
# it directly from the inspected bridge, never from logcat.
capability="${KORRI_ANDROID_DEBUG_CAPABILITY:-}"
if [[ -z "$capability" ]]; then
  capability="$("$DEBUG_CAPABILITY_SH" "$DEVICE" "$PACKAGE")"
fi

webview_errors="$(adb -s "$DEVICE" logcat -d -s WebViewAssetLoader:E)"
if grep -q 'Error opening asset path' <<<"$webview_errors"; then
  echo "WebViewAssetLoader reported missing portal assets" >&2
  exit 1
fi
console_logs="$(adb -s "$DEVICE" logcat -d)"
if grep -qE 'INFO:CONSOLE.*(blocked|Error|error)' <<<"$console_logs"; then
  echo "Portal emitted a console error" >&2
  exit 1
fi

adb -s "$DEVICE" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
adb -s "$DEVICE" forward "tcp:$HOST_PORT" "tcp:$port"
cleanup() {
  adb -s "$DEVICE" forward --remove "tcp:$HOST_PORT" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# Localhost alone is not authority: an unauthenticated caller must be rejected.
unauthorized_status="$("${CURL[@]}" --silent --output /dev/null --write-out '%{http_code}' \
  -H 'content-type: application/json' \
  -d '{"_tag":"system.health","payload":{}}' \
  "http://127.0.0.1:$HOST_PORT/rpc")"
if [[ "$unauthorized_status" != "401" ]]; then
  echo "Unauthenticated RPC returned HTTP $unauthorized_status, expected 401" >&2
  exit 1
fi

response="$("${CURL[@]}" --fail --silent \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $capability" \
  -d '{"_tag":"app.catalog.snapshot","payload":{}}' \
  "http://127.0.0.1:$HOST_PORT/rpc")"
if ! jq -e '
  .outcome._tag == "Ok"
  and any(.outcome.payload.games[]; .id == "neverball" and .host == "zao")
  and any(.outcome.payload.games[]; .host == "aka")
' <<<"$response" >/dev/null; then
  echo "Android catalog is missing the configured aka + zao hosts: $response" >&2
  exit 1
fi

# The regular device smoke must not rewrite a user's fixed config/library files.
# Installed Android route assertions are opt-in through --expect-installed-route
# after the dedicated installed-route gate has provisioned and byte-checked its
# checkpoint.
local_games_response="$("${CURL[@]}" --fail --silent \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $capability" \
  -d '{"_tag":"app.local-games.list","payload":{}}' \
  "http://127.0.0.1:$HOST_PORT/rpc")"
if [[ "$EXPECT_INSTALLED_ROUTE" == true ]]; then
  if ! jq -e '
    .outcome._tag == "Ok"
    and .outcome.payload.games[0].id == "tmnt-shredders-revenge"
    and .outcome.payload.games[0].title == "TMNT: Shredder'"'"'s Revenge"
    and .outcome.payload.games[0].system == "Android"
    and (.outcome.payload.failures | not)
  ' <<<"$local_games_response" >/dev/null; then
    echo "Local-games probe did not return the configured Android route: $local_games_response" >&2
    exit 1
  fi
  if [[ "$EXPECT_RETROARCH_ROUTE" == true ]] && ! jq -e '
    .outcome.payload.games[1].id == "wl4"
    and .outcome.payload.games[1].title == "Wario Land 4"
  ' <<<"$local_games_response" >/dev/null; then
    echo "Local-games probe did not return the canonical RetroArch route: $local_games_response" >&2
    exit 1
  fi
else
  if ! jq -e '
    .outcome._tag == "Ok"
    and (.outcome.payload.games | type == "array")
    and (.outcome.payload.failures | not)
  ' <<<"$local_games_response" >/dev/null; then
    echo "Local-games probe did not return a valid configured catalog: $local_games_response" >&2
    exit 1
  fi
fi

android_launch_response=""
if [[ "$EXPECT_INSTALLED_ROUTE" == true ]]; then
  android_launch_response="$("${CURL[@]}" --fail --silent \
    -H 'content-type: application/json' \
    -H "authorization: Bearer $capability" \
    -d '{"_tag":"app.local-games.launch","payload":{"gameId":"tmnt-shredders-revenge"}}' \
    "http://127.0.0.1:$HOST_PORT/rpc")"
  if ! require_android_app_launch_response "$android_launch_response"; then
    exit 1
  fi
fi

local_launch_response=""
if [[ "$EXPECT_INSTALLED_ROUTE" == true && "$EXPECT_RETROARCH_ROUTE" == true ]]; then
  # The dedicated checkpoint composes the @korri:retroarch launcher with the
  # independently enabled @korri:mgba runtime. Embedded Android must return a signed,
  # deferred instruction without attempting scoped-storage writes in Rust.
  local_launch_response="$("${CURL[@]}" --fail --silent \
    -H 'content-type: application/json' \
    -H "authorization: Bearer $capability" \
    -d '{"_tag":"app.local-games.launch","payload":{"gameId":"wl4"}}' \
    "http://127.0.0.1:$HOST_PORT/rpc")"
  if ! require_wl4_local_launch_response "$local_launch_response"; then
    exit 1
  fi
fi

# Session status must round-trip through the on-device brain: either a
# well-formed Ok (with or without an active session) or a tagged Err code
# — anything else means the proxy or the wire is broken.
session_response="$("${CURL[@]}" --fail --silent \
  -H 'content-type: application/json' \
  -H "authorization: Bearer $capability" \
  -d '{"_tag":"app.session.status","payload":{}}' \
  "http://127.0.0.1:$HOST_PORT/rpc")"
if ! printf '%s' "$session_response" | grep -q '"_tag":"app.session.status"'; then
  echo "Session status probe returned an unexpected shape: $session_response" >&2
  exit 1
fi
if ! printf '%s' "$session_response" | grep -qE '"_tag":"(Ok|Err)"'; then
  echo "Session status outcome is neither Ok nor Err: $session_response" >&2
  exit 1
fi

printf 'Android portal: %s\n' "$portal_ready"
printf 'Android Rust RPC: %s\n' "$response"
printf 'Android local games: %s\n' "$local_games_response"
if [[ "$EXPECT_INSTALLED_ROUTE" == true ]]; then
  printf 'Android android-app launch: %s\n' "$android_launch_response"
fi
if [[ "$EXPECT_RETROARCH_ROUTE" == true ]]; then
  printf 'Android deferred RetroArch launch: %s\n' "$local_launch_response"
fi
printf 'Android session status: %s\n' "$session_response"
printf 'Android Rust library: %s\n' "$(adb -s "$DEVICE" shell dumpsys package "$PACKAGE" | grep versionName | head -1 | xargs)"

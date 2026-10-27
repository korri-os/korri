#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ACCEPTANCE="$HERE/launcher-device-acceptance.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

[[ -x "$ACCEPTANCE" ]]
if grep -E '(^|[[:space:]])fastboot([[:space:]]|$)' "$ACCEPTANCE" >/dev/null; then
  echo 'launcher device acceptance contains fastboot' >&2
  exit 1
fi
if grep -E 'adb .*\b(reboot|push|install|uninstall|remount|root)\b|set-home-activity|settings put|pm (clear|disable|enable|install|uninstall)' "$ACCEPTANCE" >/dev/null; then
  echo 'launcher device acceptance contains a device mutation' >&2
  exit 1
fi
if "$ACCEPTANCE" wrong-serial "$TMP/wrong-serial" >"$TMP/wrong.stdout" 2>"$TMP/wrong.stderr"; then
  echo 'launcher device acceptance accepted the wrong serial' >&2
  exit 1
fi
grep -F 'device serial does not match its contract' "$TMP/wrong.stderr" >/dev/null

if [[ -z "${ODIN2PORTAL_ACCEPTANCE_SERIAL:-}" ]]; then
  printf 'odin2portal launcher device acceptance integration skipped: set ODIN2PORTAL_ACCEPTANCE_SERIAL and ODIN2PORTAL_ACCEPTANCE_EXPECT\n'
  exit 0
fi
case "${ODIN2PORTAL_ACCEPTANCE_EXPECT:-}" in
  preinstall)
    if "$ACCEPTANCE" "$ODIN2PORTAL_ACCEPTANCE_SERIAL" "$TMP/current-device" \
      >"$TMP/current.stdout" 2>"$TMP/current.stderr"; then
      echo 'launcher device acceptance unexpectedly passed before the launcher image was installed' >&2
      exit 1
    fi
    grep -F 'Korri is not installed at the approved product path' "$TMP/current.stderr" >/dev/null
    printf 'odin2portal launcher device acceptance pre-install rejection passed\n'
    ;;
  installed)
    "$ACCEPTANCE" "$ODIN2PORTAL_ACCEPTANCE_SERIAL" "$TMP/current-device" \
      >"$TMP/current.stdout" 2>"$TMP/current.stderr"
    tail -n1 "$TMP/current.stdout" | grep -Fx 'ODIN2PORTAL_LAUNCHER_DEVICE_HOST_GATES_PASS' >/dev/null
    grep -Fx 'ODIN2PORTAL_LAUNCHER_DEVICE_HOST_GATES_PASS' "$TMP/current-device/RESULT.txt" >/dev/null
    printf 'odin2portal launcher device acceptance installed-image integration passed\n'
    ;;
  *)
    echo 'ODIN2PORTAL_ACCEPTANCE_EXPECT must be preinstall or installed when a serial is set' >&2
    exit 1
    ;;
esac

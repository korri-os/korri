#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
READINESS="$HERE/install-readiness.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

[[ -x "$READINESS" ]]
if grep -E '(^|[[:space:]])(adb|fastboot)([[:space:]]|$)' "$READINESS" >/dev/null; then
  echo 'install readiness check contains a device command' >&2
  exit 1
fi
mkdir "$TMP/signed" "$TMP/rollback"
if "$READINESS" "$TMP/signed" "$TMP/rollback" >"$TMP/incomplete.stdout" 2>"$TMP/incomplete.stderr"; then
  echo 'install readiness accepted incomplete inputs' >&2
  exit 1
fi
grep -F 'signed output is incomplete or symbolic' "$TMP/incomplete.stderr" >/dev/null
ln -s "$TMP/signed" "$TMP/signed-link"
if "$READINESS" "$TMP/signed-link" "$TMP/rollback" >"$TMP/symlink.stdout" 2>"$TMP/symlink.stderr"; then
  echo 'install readiness accepted a symbolic input directory' >&2
  exit 1
fi
grep -F 'input directory is missing or symbolic' "$TMP/symlink.stderr" >/dev/null

if [[ -z "${ODIN2PORTAL_SIGNED_OUTPUT:-}" || -z "${ODIN2PORTAL_ROLLBACK_BUNDLE:-}" ]]; then
  printf 'odin2portal install readiness integration skipped: set both artifact variables\n'
  exit 0
fi
"$READINESS" "$ODIN2PORTAL_SIGNED_OUTPUT" "$ODIN2PORTAL_ROLLBACK_BUNDLE" \
  > "$TMP/readiness.txt"
tail -n 1 "$TMP/readiness.txt" | grep -Fx 'ODIN2PORTAL_INSTALL_ARTIFACTS_READY' >/dev/null
grep -Fx 'device writes: none' "$TMP/readiness.txt" >/dev/null
grep -Fx 'installation approved: no' "$TMP/readiness.txt" >/dev/null
printf 'odin2portal install readiness integration passed\n'

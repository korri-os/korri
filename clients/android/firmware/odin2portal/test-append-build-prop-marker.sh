#!/usr/bin/env bash
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPEND="$HERE/append-build-prop-marker.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/root/etc"
printf 'ro.fixture=true\n# end of file\n' > "$TMP/root/etc/build.prop"
printf '# korri marker-only dry run\n' > "$TMP/marker.txt"
truncate -s 33554432 "$TMP/product.img"
mke2fs -q -F -t ext4 -L product -d "$TMP/root" "$TMP/product.img"

before_inode="$(debugfs -R 'stat /etc/build.prop' "$TMP/product.img" 2>/dev/null | awk '/^Inode:/ {print $2}')"
before_mode="$(debugfs -R 'stat /etc/build.prop' "$TMP/product.img" 2>/dev/null | awk '/^Inode:/ {print $6}')"
"$APPEND" "$TMP/product.img" /etc/build.prop "$TMP/marker.txt"
after_inode="$(debugfs -R 'stat /etc/build.prop' "$TMP/product.img" 2>/dev/null | awk '/^Inode:/ {print $2}')"
after_mode="$(debugfs -R 'stat /etc/build.prop' "$TMP/product.img" 2>/dev/null | awk '/^Inode:/ {print $6}')"
[[ "$before_inode" == "$after_inode" ]]
[[ "$before_mode" == "$after_mode" ]]
debugfs -R 'cat /etc/build.prop' "$TMP/product.img" 2>/dev/null > "$TMP/actual-build.prop"
printf 'ro.fixture=true\n# end of file\n# korri marker-only dry run\n' > "$TMP/expected-build.prop"
cmp "$TMP/expected-build.prop" "$TMP/actual-build.prop"
e2fsck -fn "$TMP/product.img" >/dev/null

if "$APPEND" "$TMP/product.img" /etc/build.prop "$TMP/marker.txt" >"$TMP/duplicate.stdout" 2>"$TMP/duplicate.stderr"; then
  echo 'marker append accepted a duplicate marker' >&2
  exit 1
fi
grep -F 'marker is already present' "$TMP/duplicate.stderr" >/dev/null

mkdir -p "$TMP/no-newline-root/etc"
printf 'ro.fixture=true' > "$TMP/no-newline-root/etc/build.prop"
truncate -s 33554432 "$TMP/no-newline.img"
mke2fs -q -F -t ext4 -L product -d "$TMP/no-newline-root" "$TMP/no-newline.img"
if "$APPEND" "$TMP/no-newline.img" /etc/build.prop "$TMP/marker.txt" >"$TMP/no-newline.stdout" 2>"$TMP/no-newline.stderr"; then
  echo 'marker append accepted build.prop without a trailing newline' >&2
  exit 1
fi
grep -F 'target does not end with a newline; marker would not be a comment' "$TMP/no-newline.stderr" >/dev/null

printf 'ro.korri.marker=true\n' > "$TMP/property-marker.txt"
if "$APPEND" "$TMP/product.img" /etc/build.prop "$TMP/property-marker.txt" >"$TMP/property.stdout" 2>"$TMP/property.stderr"; then
  echo 'marker append accepted a property assignment' >&2
  exit 1
fi
grep -F 'marker must contain only newline-terminated comment lines' "$TMP/property.stderr" >/dev/null

[[ -x "$HERE/marker-dry-run.sh" ]]
if grep -E '(^|[[:space:]])(adb|fastboot)([[:space:]]|$)' "$HERE/marker-dry-run.sh" >/dev/null; then
  echo 'marker dry run contains a device command' >&2
  exit 1
fi

printf 'odin2portal build.prop marker tests passed\n'

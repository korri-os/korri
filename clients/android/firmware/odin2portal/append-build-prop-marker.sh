#!/usr/bin/env bash
set -euo pipefail

IMAGE="${1:?usage: append-build-prop-marker.sh <ext4-image> <filesystem-path> <marker-file>}"
FILESYSTEM_PATH="${2:?usage: append-build-prop-marker.sh <ext4-image> <filesystem-path> <marker-file>}"
MARKER="${3:?usage: append-build-prop-marker.sh <ext4-image> <filesystem-path> <marker-file>}"

for file in "$IMAGE" "$MARKER"; do
  if [[ ! -f "$file" || -L "$file" ]]; then
    echo "input is missing, not regular, or symbolic: $file" >&2
    exit 1
  fi
done
if [[ ! -s "$MARKER" ]]; then
  echo 'marker file is empty' >&2
  exit 1
fi
marker_newline="$(mktemp)"
printf '\n' > "$marker_newline"
trap 'rm -f "$marker_newline"' EXIT
if ! tail -c 1 "$MARKER" | cmp -s "$marker_newline" - ||
   grep -Ev '^#.*$' "$MARKER" >/dev/null ||
   grep -F $'\r' "$MARKER" >/dev/null; then
  echo 'marker must contain only newline-terminated comment lines' >&2
  exit 1
fi
if tune2fs -l "$IMAGE" 2>/dev/null | grep '^Filesystem features:' | grep -qw shared_blocks; then
  echo 'filesystem still contains shared blocks' >&2
  exit 1
fi

before="$(mktemp)"
after="$(mktemp)"
newline="$(mktemp)"
printf '\n' > "$newline"
trap 'rm -f "$marker_newline" "$before" "$after" "$newline"' EXIT
debugfs -R "cat $FILESYSTEM_PATH" "$IMAGE" > "$before" 2>/dev/null
if grep -F -x -f "$MARKER" "$before" >/dev/null; then
  echo 'marker is already present' >&2
  exit 1
fi
if ! tail -c 1 "$before" | cmp -s "$newline" -; then
  echo 'target does not end with a newline; marker would not be a comment' >&2
  exit 1
fi

file_size="$(debugfs -R "stat $FILESYSTEM_PATH" "$IMAGE" 2>/dev/null | awk '/Size:/ {for (i=1; i<=NF; i++) if ($i == "Size:") {print $(i+1); exit}}')"
read -r -a data_blocks <<<"$(debugfs -R "blocks $FILESYSTEM_PATH" "$IMAGE" 2>/dev/null)"
block_size="$(tune2fs -l "$IMAGE" 2>/dev/null | awk -F: '/^Block size:/ {gsub(/ /, "", $2); print $2}')"
marker_size="$(stat -c %s "$MARKER")"
if [[ ! "$file_size" =~ ^[0-9]+$ || ! "$block_size" =~ ^[0-9]+$ ||
      "${#data_blocks[@]}" -ne 1 || ! "${data_blocks[0]}" =~ ^[0-9]+$ ]]; then
  echo 'target must be a regular file stored in exactly one data block' >&2
  exit 1
fi
if ((file_size + marker_size > block_size)); then
  echo 'marker does not fit in the existing target data block' >&2
  exit 1
fi

dd if="$MARKER" of="$IMAGE" bs=1 \
  seek=$((data_blocks[0] * block_size + file_size)) \
  conv=notrunc status=none
debugfs -w -R \
  "set_inode_field $FILESYSTEM_PATH size $((file_size + marker_size))" \
  "$IMAGE" >/dev/null

e2fsck -fy "$IMAGE" >/dev/null
e2fsck -fn "$IMAGE" >/dev/null
debugfs -R "cat $FILESYSTEM_PATH" "$IMAGE" > "$after" 2>/dev/null
tail -c "$marker_size" "$after" | cmp -s "$MARKER" - || {
  echo 'marker verification failed after append' >&2
  exit 1
}
printf 'BUILD_PROP_MARKER_APPENDED path=%s bytes=%s\n' "$FILESYSTEM_PATH" "$marker_size"

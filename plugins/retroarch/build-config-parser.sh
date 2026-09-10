#!/usr/bin/env bash
# Build-machine only. Compile the real parser and its real file/path adapters.
set -euo pipefail
source_root="$1/libretro-common"
probe="$2"
output="$3"
"${CC:-cc}" -std=gnu99 -O2 -ffunction-sections -fdata-sections \
  -I"$source_root/include" \
  "$probe" \
  "$source_root/file/config_file.c" \
  "$source_root/file/file_path.c" \
  "$source_root/file/file_path_io.c" \
  "$source_root/streams/file_stream.c" \
  "$source_root/streams/file_stream_transforms.c" \
  "$source_root/vfs/vfs_implementation.c" \
  "$source_root/string/stdstring.c" \
  "$source_root/compat/compat_strl.c" \
  "$source_root/compat/compat_posix_string.c" \
  "$source_root/encodings/encoding_utf.c" \
  "$source_root/time/rtime.c" \
  -Wl,--gc-sections -o "$output"

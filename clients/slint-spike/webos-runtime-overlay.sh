#!/usr/bin/env nix
#! nix shell nixpkgs#bash nixpkgs#coreutils nixpkgs#gnugrep nixpkgs#util-linux --command bash
# shellcheck shell=bash
# Local only: apply checked public target libraries to a private SDK sysroot.
# No device access, target execution, firmware library distribution, or host patching.
set -euo pipefail
if [[ $# -ne 4 ]]; then
  echo 'usage: webos-runtime-overlay.sh SDK_DIR TV_LIBRARY_DIR SDK_ARCHIVE TARGET_DIR' >&2
  exit 2
fi
sdk="$(realpath -e "$1")"
runtime="$(realpath -e "$2")"
archive="$(realpath -e "$3")"
target="$(realpath -e "$4")"
sysroot="$sdk/arm-webos-linux-gnueabi/sysroot"
readelf="$sdk/bin/arm-webos-linux-gnueabi-readelf"
expected_archive=2230a6351e5cce6fcb9d4fdcb4697335f223995401ef969c81744aaeb14c5344
fail() { echo "webos-runtime-overlay: $*" >&2; exit 1; }
[[ -f "$sdk/relocate-sdk.sh" && -x "$readelf" ]] || fail 'not a prepared webOS SDK'
[[ -d "$sysroot/lib" && ! -L "$sysroot/lib" ]] || fail 'sysroot/lib must be a real directory'
[[ -f "$runtime/SHA256SUMS" && ! -L "$runtime/SHA256SUMS" ]] || fail 'completed SHA256SUMS is missing'
[[ -d "$target" ]] || fail 'target cache directory is missing'
# Keep the original verified archive unchanged as the complete SDK recovery source.
printf '%s  %s\n' "$expected_archive" "$archive" | sha256sum --check --strict

# This is the exact public /lib set fetched by the parent from the measured TV.
libraries=(libc.so.6 ld-linux.so.3 libpthread.so.0 libm.so.6 libdl.so.2 librt.so.1 libutil.so.1 libresolv.so.2)
declare -A seen=()
while read -r digest name extra; do
  [[ "$digest" =~ ^[[:xdigit:]]{64}$ && -z "$extra" ]] || fail 'malformed checksum entry'
  name="${name#\*}"
  case "$name" in
    libc.so.6|ld-linux.so.3|libpthread.so.0|libm.so.6|libdl.so.2|librt.so.1|libutil.so.1|libresolv.so.2) ;;
    *) fail "unapproved checksum entry: $name" ;;
  esac
  [[ -z "${seen[$name]:-}" ]] || fail "duplicate checksum entry: $name"
  seen[$name]=1
done < "$runtime/SHA256SUMS"
[[ ${#seen[@]} -eq ${#libraries[@]} ]] || fail 'all eight public-library checksums are required'
for name in "${libraries[@]}"; do
  [[ -f "$runtime/$name" && ! -L "$runtime/$name" ]] || fail "input must be a checked regular file: $name"
done
(cd "$runtime" && sha256sum --check --strict SHA256SUMS)

# Same writer lock as the cross task, including when TARGET_DIR is a symlink.
exec 9>"$target/.writer.lock"
flock -n 9 || fail 'a cross build already owns the target cache'
backup="$sdk/.korri-runtime-original"
[[ ! -e "$backup" ]] || fail "original runtime backup already exists: $backup"
stage="$(mktemp -d "$sdk/.korri-runtime-stage.XXXXXXXX")"
trap 'rm -rf "$stage"' EXIT
for name in "${libraries[@]}"; do
  install -m 0644 "$runtime/$name" "$stage/$name"
  header="$("$readelf" --file-header "$stage/$name")"
  [[ "$header" == *'ELF32'* && "$header" == *'Machine:'*'ARM'* && "$header" == *'Version5 EABI'* && "$header" == *'soft-float ABI'* ]] \
    || fail "not ARM ELF32 EABI soft-float: $name"
  dynamic="$("$readelf" --dynamic "$stage/$name")"
  [[ "$dynamic" == *"Library soname: [$name]"* ]] || fail "SONAME mismatch: $name"
done
cp "$runtime/SHA256SUMS" "$stage/SHA256SUMS"
(cd "$stage" && sha256sum --check --strict SHA256SUMS)
# This missing real glibc symbol caused the measured ring/libstd link failure.
"$readelf" --dyn-syms --wide "$stage/libc.so.6" > "$stage/libc-symbols.txt"
grep -q ' getauxval@@GLIBC_2.16' "$stage/libc-symbols.txt" || fail 'libc does not export getauxval@GLIBC_2.16'

# Preserve regular copies of all old SONAME contents before any replacement.
# Old versioned library files and the original archive also remain untouched.
mkdir "$backup"
for name in "${libraries[@]}"; do
  cp --dereference --preserve=mode "$sysroot/lib/$name" "$backup/$name"
done
(cd "$backup" && sha256sum "${libraries[@]}" > SHA256SUMS)
for name in "${libraries[@]}"; do
  # Unlink first. Never write through a SONAME symlink into an old versioned file.
  rm -- "$sysroot/lib/$name"
  mv -- "$stage/$name" "$sysroot/lib/$name"
done
cp "$stage/SHA256SUMS" "$sdk/.korri-tv-runtime-SHA256SUMS"
(cd "$sysroot/lib" && sha256sum --check --strict "$sdk/.korri-tv-runtime-SHA256SUMS")
printf 'Public TV runtime applied only to %s/lib\nOriginal runtime backup: %s\n' "$sysroot" "$backup"
echo 'SDK headers and nonshared archives are unchanged. No library was shipped or copied to a device.'

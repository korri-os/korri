#!/usr/bin/env bash
# Called by webos.nix. Workstation only; no target execution or deployment.
set -euo pipefail
action="${1:?expected probe, korrid, or build}"
shift
case "$action" in probe|korrid|build) ;; *) echo "Unknown action: $action" >&2; exit 2 ;; esac
: "${KORRI_ROOT:?run via webos.nix}"
: "${WEBOS_HOST_LOADER:?run via webos.nix}"
: "${WEBOS_HOST_LIBRARIES:?run via webos.nix}"
sdk="${WEBOS_SDK:-$KORRI_ROOT/.cache/webos-sdk}"
sdk="$(realpath "$sdk")"
prefix="$sdk/bin/arm-webos-linux-gnueabi"
if [[ ! -x "$prefix-gcc" ]]; then
  echo "SDK missing: extract the verified archive with clients/slint-spike/webos-sdk.sh" >&2
  exit 1
fi
export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/webos-target"
mkdir -p "$CARGO_TARGET_DIR"
# No concurrent writer may use this target directory, even for another action.
exec 9>"$CARGO_TARGET_DIR/.writer.lock"
flock -n 9 || { echo "Another webOS build owns $CARGO_TARGET_DIR" >&2; exit 1; }

# Patch only workstation x86_64 ELF executables in this private SDK extraction.
# Never alter ARM libraries, the target loader, or anything on a device.
if [[ ! -f "$sdk/.korri-host-loader" || "$(cat "$sdk/.korri-host-loader")" != "$WEBOS_HOST_LOADER" ]]; then
  while IFS= read -r -d '' executable; do
    if file -b "$executable" | grep -q 'ELF 64-bit.*x86-64.*interpreter'; then
      patchelf --set-interpreter "$WEBOS_HOST_LOADER" "$executable"
      existing="$(patchelf --print-rpath "$executable")"
      patchelf --set-rpath "$WEBOS_HOST_LIBRARIES${existing:+:$existing}" "$executable"
    fi
  done < <(find "$sdk/bin" "$sdk/libexec" -type f -perm /111 -print0)
  printf '%s\n' "$WEBOS_HOST_LOADER" > "$sdk/.korri-host-loader"
fi
sysroot="$("$prefix-gcc" -print-sysroot)"
export CC_armv7_unknown_linux_gnueabi="$prefix-gcc"
export CXX_armv7_unknown_linux_gnueabi="$prefix-g++"
export AR_armv7_unknown_linux_gnueabi="$prefix-ar"
export CARGO_TARGET_ARMV7_UNKNOWN_LINUX_GNUEABI_LINKER="$prefix-gcc"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
if [[ ! "$CARGO_BUILD_JOBS" =~ ^[12]$ ]]; then
  echo "CARGO_BUILD_JOBS must be 1 or 2 for this bounded spike" >&2
  exit 2
fi
# Bindgen must see target headers and the soft-float calling ABI, not host glibc.
export BINDGEN_EXTRA_CLANG_ARGS_armv7_unknown_linux_gnueabi="--sysroot=$sysroot --target=armv7-unknown-linux-gnueabi -mfloat-abi=softfp -I$sysroot/usr/include"
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_SYSROOT_DIR_armv7_unknown_linux_gnueabi="$sysroot"
# Do not let an inherited workstation PKG_CONFIG_PATH add host .pc files.
export PKG_CONFIG_PATH_armv7_unknown_linux_gnueabi=
export PKG_CONFIG_LIBDIR_armv7_unknown_linux_gnueabi="$sysroot/usr/lib/pkgconfig:$sysroot/usr/share/pkgconfig"
export KORRI_BUILD_ID
KORRI_BUILD_ID="$(git -C "$KORRI_ROOT" rev-parse --short=12 HEAD)"
source_rev="$(git -C "$KORRI_ROOT" rev-parse HEAD)"
if [[ -n "$(git -C "$KORRI_ROOT" status --porcelain)" ]]; then
  KORRI_BUILD_ID="$KORRI_BUILD_ID-dirty"
  source_rev="$source_rev-dirty"
fi
printf 'Source: %s\nSDK: %s\nSysroot: %s\nTarget: %s\n' \
  "$source_rev" "$sdk" "$sysroot" armv7-unknown-linux-gnueabi
case "$action" in
  probe)
    probe="$CARGO_TARGET_DIR/abi-probe"
    mkdir -p "$probe"
    "$prefix-gcc" "$KORRI_ROOT/clients/slint-spike/webos-abi-probe.c" -o "$probe/hello"
    file "$probe/hello"
    "$prefix-readelf" -h -l -A -V "$probe/hello" | tee "$probe/readelf.txt"
    grep -q 'soft-float ABI' "$probe/readelf.txt"
    grep -q '/lib/ld-linux.so.3' "$probe/readelf.txt"
    echo 'ARM ABI PROBE PASS (static inspection only; not target execution)'
    ;;
  korrid)
    exec cargo build --locked --manifest-path "$KORRI_ROOT/services/korrid/Cargo.toml" \
      --lib --target armv7-unknown-linux-gnueabi "$@"
    ;;
  build)
    export CARGO_PROFILE_RELEASE_STRIP=symbols
    exec cargo build --locked --release --manifest-path "$KORRI_ROOT/clients/slint-spike/Cargo.toml" \
      --no-default-features --features webos --target armv7-unknown-linux-gnueabi "$@"
    ;;
esac

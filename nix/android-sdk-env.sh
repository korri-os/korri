#!/usr/bin/env bash
# Materialise the writable Android SDK layout AGP expects and export its paths.
# This file is sourced by devshells and Nix tasks because apps do not run
# shellHooks.
#
# Expects KORRI_NIX_SDK, KORRI_NDK_VERSION, and KORRI_ANDROID_SDK_NAME.
# KORRI_ROOT defaults to $PWD.

korri_android_sdk_env() {
  # Keep strict options local: this file is also sourced into interactive shells.
  local -
  set -euo pipefail

  : "${KORRI_NIX_SDK:?android-sdk-env.sh needs KORRI_NIX_SDK}"
  : "${KORRI_NDK_VERSION:?android-sdk-env.sh needs KORRI_NDK_VERSION}"
  : "${KORRI_ANDROID_SDK_NAME:?android-sdk-env.sh needs KORRI_ANDROID_SDK_NAME}"

  local root="${KORRI_ROOT:-$PWD}"
  local sdk_dir="$root/.android-sdk/$KORRI_ANDROID_SDK_NAME"
  local marker="$sdk_dir/.korri-nix-sdk"
  local item name platform license

  if [ ! -d "$KORRI_NIX_SDK/ndk-bundle" ]; then
    printf 'android-sdk-env.sh: expected NDK at %s/ndk-bundle\n' "$KORRI_NIX_SDK" >&2
    return 1
  fi

  if [[ ! -d "$sdk_dir" || ! -e "$sdk_dir/ndk/$KORRI_NDK_VERSION" || ! -f "$marker" || "$(<"$marker")" != "$KORRI_NIX_SDK" ]]; then
    rm -rf "$sdk_dir"
    mkdir -p "$sdk_dir"

    for item in "$KORRI_NIX_SDK"/*; do
      name="$(basename "$item")"
      case "$name" in
        # `ndk` must be a real directory so the version link below does not try
        # to write through a symlink into the read-only Nix store.
        platforms | ndk | ndk-bundle | licenses) continue ;;
        *) ln -sf "$item" "$sdk_dir/$name" ;;
      esac
    done

    mkdir -p "$sdk_dir/platforms"
    for platform in "$KORRI_NIX_SDK/platforms"/*; do
      ln -sf "$platform" "$sdk_dir/platforms/$(basename "$platform")"
    done

    # Nix ships android-36.1 while AGP asks for android-36.
    if [ -d "$KORRI_NIX_SDK/platforms/android-36.1" ] && [ ! -e "$sdk_dir/platforms/android-36" ]; then
      ln -sf "$KORRI_NIX_SDK/platforms/android-36.1" "$sdk_dir/platforms/android-36"
    fi

    mkdir -p "$sdk_dir/ndk"
    ln -sf "$KORRI_NIX_SDK/ndk-bundle" "$sdk_dir/ndk/$KORRI_NDK_VERSION"

    # Licences must be writable real files because Gradle reads and rewrites them.
    mkdir -p "$sdk_dir/licenses"
    if [ -d "$KORRI_NIX_SDK/licenses" ]; then
      for license in "$KORRI_NIX_SDK/licenses"/*; do
        cat "$license" > "$sdk_dir/licenses/$(basename "$license")"
      done
    fi

    printf '%s\n' "$KORRI_NIX_SDK" > "$marker"
  fi

  export ANDROID_HOME="$sdk_dir"
  export ANDROID_SDK_ROOT="$sdk_dir"
  export ANDROID_NDK_ROOT="$sdk_dir/ndk/$KORRI_NDK_VERSION"
  export PATH="$ANDROID_SDK_ROOT/platform-tools:$PATH"
}

korri_android_sdk_env
korri_android_sdk_env_status=$?
unset -f korri_android_sdk_env
return "$korri_android_sdk_env_status" 2>/dev/null || exit "$korri_android_sdk_env_status"

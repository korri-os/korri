#!/usr/bin/env nix-shell
#!nix-shell -i bash -p nix git
# Run through the Nix apps, which supply the pinned Android and Rust toolchains.
set -euo pipefail
: "${KORRI_ROOT:=$(git rev-parse --show-toplevel)}"
source "$KORRI_ROOT/nix/android-sdk-env.sh"
mode="${1:?usage: build.sh apk|test|brain-check|device-test-apk}"
shift

ndk_bin="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CC_aarch64_linux_android="$ndk_bin/aarch64-linux-android22-clang"
export AR_aarch64_linux_android="$ndk_bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export BINDGEN_EXTRA_CLANG_ARGS_aarch64_linux_android="--target=aarch64-linux-android22 --sysroot=$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/sysroot"
export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/android-korrid-target"

case "$mode" in
  brain-check)
    cd "$KORRI_ROOT/services/korrid"
    exec cargo check --locked --lib --target aarch64-linux-android "$@"
    ;;
  apk)
    cd "$KORRI_ROOT/services/korrid"
    cargo build --locked --release --lib --target aarch64-linux-android
    install -Dm755 "$CARGO_TARGET_DIR/aarch64-linux-android/release/libkorrid.so" \
      "$KORRI_ROOT/clients/android/app/src/main/jniLibs/arm64-v8a/libkorrid.so"
    "$KORRI_PORTAL_BUNDLE"
    rm -rf "$KORRI_ROOT/clients/android/app/src/main/assets/portal"
    mkdir -p "$KORRI_ROOT/clients/android/app/src/main/assets/portal"
    cp -R "$KORRI_ROOT/clients/portal/dist/." "$KORRI_ROOT/clients/android/app/src/main/assets/portal/"
    cd "$KORRI_ROOT/clients/android"
    exec ./gradlew assembleDebug "$@"
    ;;
  device-test-apk)
    cd "$KORRI_ROOT/clients/android"
    exec ./gradlew :app:assembleDebugAndroidTest "$@"
    ;;
  test)
    cd "$KORRI_ROOT/clients/android"
    exec ./gradlew :app:testDebugUnitTest "$@"
    ;;
  *) echo "Unknown Android build mode: $mode" >&2; exit 2 ;;
esac

# Korri task definitions. Runnable apps and generated help derive from the
# same definitions so the command surface cannot drift from its documentation.
{
  pkgs,
  proseql,
  extraHelpText ? "",
}:
let
  proseqlSource = import ../services/korrid/proseql-source.nix { inherit pkgs proseql; };
  android = import ../clients/android/sdk.nix { inherit pkgs; };
  androidBridgeEmulator = import ../clients/android/sdk.nix {
    inherit pkgs;
    bridgeEmulatorProfile = true;
  };

  rustToolchain = pkgs.rust-bin.stable.latest.default.override {
    targets = [ "aarch64-linux-android" ];
  };
  androidBridgeRustToolchain = pkgs.rust-bin.stable.latest.default.override {
    targets = [ "x86_64-linux-android" ];
  };

  retroarch = import ../plugins/retroarch/android/sdk.nix { inherit pkgs; };
  odin2portalStockRepack = import ../clients/android/firmware/odin2portal/sdk.nix { inherit pkgs; };

  androidInputs = [
    android.jdk
    android.androidSdk
    pkgs.coreutils
    pkgs.file
    pkgs.git
    pkgs.gnumake
    pkgs.unzip
    pkgs.which
  ];
  androidEnv = {
    JAVA_HOME = "${android.jdk}";
    GRADLE_OPTS = android.gradleOpts;
    KORRI_ANDROID_SDK_NAME = "android";
    KORRI_NIX_SDK = android.sdkRoot;
    KORRI_NDK_VERSION = android.ndkVersion;
  };
  androidSetup = ''
    # shellcheck source=/dev/null
    source "$KORRI_ROOT/nix/android-sdk-env.sh"
  '';
  androidBridgeInputs = androidInputs ++ [
    androidBridgeEmulator.androidSdk
    androidBridgeRustToolchain
    pkgs.android-tools
    pkgs.bun
    pkgs.cargo-ndk
    pkgs.clang
    pkgs.gawk
    pkgs.gnugrep
    pkgs.gnused
    pkgs.llvmPackages.libclang
    pkgs.util-linux
  ];
  androidBridgeEnv = androidEnv // {
    ANDROID_NDK_HOME = android.ndkRoot;
    ANDROID_NDK_ROOT = android.ndkRoot;
    BINDGEN_EXTRA_CLANG_ARGS = "--target=x86_64-linux-android21 --sysroot=${android.ndkRoot}/toolchains/llvm/prebuilt/linux-x86_64/sysroot";
    BINDGEN_EXTRA_CLANG_ARGS_x86_64_linux_android = "--target=x86_64-linux-android21 --sysroot=${android.ndkRoot}/toolchains/llvm/prebuilt/linux-x86_64/sysroot";
    CC_x86_64_unknown_linux_gnu = "${pkgs.clang}/bin/clang";
    HOST_CC = "${pkgs.clang}/bin/clang";
    KORRI_BRIDGE_AVD_PACKAGE = androidBridgeEmulator.bridgeEmulatorAvdPackage;
    KORRI_EMULATOR_NIX_SDK = androidBridgeEmulator.sdkRoot;
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  };

  retroarchInputs = [
    retroarch.jdk
    retroarch.androidSdk
    pkgs.cmake
    pkgs.coreutils
    pkgs.diffutils
    pkgs.file
    pkgs.git
    pkgs.gnugrep
    pkgs.gnumake
    pkgs.gnused
    pkgs.ninja
    pkgs.unzip
    pkgs.which
  ];
  retroarchSigningInputs = [
    retroarch.jdk
    retroarch.androidSdk
    pkgs.coreutils
    pkgs.gnused
  ];
  retroarchEnv = {
    JAVA_HOME = "${retroarch.jdk}";
    GRADLE_OPTS = retroarch.gradleOpts;
    KORRI_ANDROID_SDK_NAME = "retroarch";
    KORRI_NIX_SDK = retroarch.sdkRoot;
    KORRI_NDK_VERSION = retroarch.ndkVersion;
  };
  retroarchSetup = ''
    # shellcheck source=/dev/null
    source "$KORRI_ROOT/nix/android-sdk-env.sh"
  '';

  adbPreflight = ''
    if [[ "$serial" == *:* ]]; then
      timeout 15 adb connect "$serial" >/dev/null || true
    fi
    if ! timeout 15 adb -s "$serial" wait-for-device; then
      echo "Android target is not reachable: $serial" >&2
      exit 1
    fi
  '';

  deviceScript =
    name: file: task:
    task
    // {
      usageSuffix = task.usageSuffix or " -- <adb-serial>";
      runtimeInputs = [
        pkgs.android-tools
        pkgs.coreutils
      ]
      ++ (task.runtimeInputs or [ ]);
      script = ''
        serial="''${1:?usage: ${name} <adb-serial>}"
        ${adbPreflight}
        ${task.setup or ""}
        exec "$KORRI_ROOT/services/korrid/${file}" "$@"
      '';
    };

  definitions = {
    android-apk = {
      description = "Build the debug Android APK (run portal-bundle first for bundled assets).";
      runtimeInputs = androidInputs;
      env = androidEnv;
      script = ''
        ${androidSetup}
        cd "$KORRI_ROOT/clients/android"
        exec ./gradlew assembleDebug "$@"
      '';
    };

    android-apk-dev = {
      description = "Build the debug APK against a live portal URL.";
      runtimeInputs = androidInputs;
      env = androidEnv;
      usageSuffix = " -- <portal-url>";
      script = ''
        url="''${1:?usage: android-apk-dev <portal-url>}"
        shift
        ${androidSetup}
        cd "$KORRI_ROOT/clients/android"
        exec ./gradlew assembleDebug "-PkorriPortalUrl=$url" "$@"
      '';
    };

    android-jvm-check = {
      description = "Run the Android debug JVM/Robolectric test suite.";
      runtimeInputs = androidInputs;
      env = androidEnv;
      script = ''
        ${androidSetup}
        cd "$KORRI_ROOT/clients/android"
        exec ./gradlew testDebugUnitTest "$@"
      '';
    };

    odin2portal-marker-dry-run = {
      description = "Build and verify a non-flashable one-comment Odin 2 Portal image mutation.";
      runtimeInputs = odin2portalStockRepack.markerRuntimeInputs;
      usageSuffix = " -- <stock-source-directory> <output-directory>";
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/marker-dry-run.sh" "$@"
      '';
    };

    odin2portal-marker-dry-run-check = {
      description = "Test the Odin 2 Portal marker mutation and, when configured, its full private-source pipeline.";
      runtimeInputs = odin2portalStockRepack.markerRuntimeInputs;
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/test-marker-dry-run.sh"
      '';
    };

    odin2portal-launcher-image-dry-run = {
      description = "Build and host-verify a quarantined Odin 2 Portal Korri launcher image.";
      runtimeInputs = androidInputs ++ odin2portalStockRepack.launcherRuntimeInputs;
      env = androidEnv;
      usageSuffix = " -- <stock-source> <signed-arm64-release-apk> <private-avb-key> <output>";
      script = ''
        ${androidSetup}
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/launcher-image-dry-run.sh" "$@"
      '';
    };

    odin2portal-launcher-image-dry-run-check = {
      description = "Test Korri launcher manifest, APK, quarantine, and host-only image guards.";
      runtimeInputs = androidInputs ++ odin2portalStockRepack.launcherRuntimeInputs;
      env = androidEnv;
      script = ''
        ${androidSetup}
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/test-launcher-image-dry-run.sh"
      '';
    };

    odin2portal-launcher-install-readiness = {
      description = "Verify the exact Korri launcher and stock rollback artifacts without device writes.";
      runtimeInputs = odin2portalStockRepack.installRuntimeInputs;
      usageSuffix = " -- <launcher-output> <rollback-bundle>";
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/launcher-install-readiness.sh" "$@"
      '';
    };

    odin2portal-launcher-install-readiness-check = {
      description = "Test the host-only Odin 2 Portal launcher installation readiness gate.";
      runtimeInputs = odin2portalStockRepack.installRuntimeInputs;
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/test-launcher-install-readiness.sh"
      '';
    };

    odin2portal-launcher-device-acceptance = {
      description = "Verify the installed Odin launcher image and signer without device writes.";
      runtimeInputs = androidInputs ++ [ pkgs.android-tools pkgs.gnugrep pkgs.gnused ];
      env = androidEnv;
      usageSuffix = " -- <adb-serial> <evidence-directory>";
      script = ''
        ${androidSetup}
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/launcher-device-acceptance.sh" "$@"
      '';
    };

    odin2portal-launcher-device-acceptance-check = {
      description = "Test launcher device acceptance safety and optional pre-install rejection.";
      runtimeInputs = androidInputs ++ [ pkgs.android-tools pkgs.gnugrep pkgs.gnused ];
      env = androidEnv;
      script = ''
        ${androidSetup}
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/test-launcher-device-acceptance.sh"
      '';
    };

    odin2portal-signed-avb-dry-run = {
      description = "Build and verify a non-flashable signed AVB chain for the Odin 2 Portal marker image.";
      runtimeInputs = odin2portalStockRepack.signedAvbRuntimeInputs;
      usageSuffix = " -- <stock-source> <private-key> <output>";
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/signed-avb-dry-run.sh" "$@"
      '';
    };

    odin2portal-signed-avb-dry-run-check = {
      description = "Test the Odin 2 Portal signed AVB dry-run guards and optional private-source pipeline.";
      runtimeInputs = odin2portalStockRepack.signedAvbRuntimeInputs;
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/test-signed-avb-dry-run.sh"
      '';
    };

    odin2portal-install-readiness = {
      description = "Verify signed and rollback artifacts for the non-executing Odin 2 Portal installation plan.";
      runtimeInputs = odin2portalStockRepack.installRuntimeInputs;
      usageSuffix = " -- <signed-avb-output> <rollback-bundle>";
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/install-readiness.sh" "$@"
      '';
    };

    odin2portal-install-readiness-check = {
      description = "Test the host-only Odin 2 Portal installation readiness gate.";
      runtimeInputs = odin2portalStockRepack.installRuntimeInputs;
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/test-install-readiness.sh"
      '';
    };

    odin2portal-rollback-bundle = {
      description = "Stage the exact Odin 2 Portal stock Android rollback images without device writes.";
      runtimeInputs = odin2portalStockRepack.rollbackRuntimeInputs;
      usageSuffix = " -- <stock-source-directory> <output-directory>";
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/rollback-bundle.sh" "$@"
      '';
    };

    odin2portal-rollback-bundle-check = {
      description = "Test the fail-closed, non-executing Odin 2 Portal rollback bundle staging.";
      runtimeInputs = odin2portalStockRepack.rollbackRuntimeInputs;
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/test-rollback-bundle.sh"
      '';
    };

    odin2portal-stock-repack = {
      description = "Reconstruct and verify a stock-equivalent Odin 2 Portal super image without device writes.";
      runtimeInputs = odin2portalStockRepack.runtimeInputs;
      usageSuffix = " -- <stock-source-directory> <output-directory>";
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/repack.sh" "$@"
      '';
    };

    odin2portal-stock-repack-check = {
      description = "Test the fail-closed, read-only Odin 2 Portal stock reconstruction pipeline.";
      runtimeInputs = odin2portalStockRepack.runtimeInputs;
      script = ''
        exec "$KORRI_ROOT/clients/android/firmware/odin2portal/test-repack.sh"
      '';
    };

    android-bridge-contract-check = {
      description = "Run the native bridge contract check in an isolated API 34 x86_64 emulator.";
      needsProseql = true;
      runtimeInputs = androidBridgeInputs;
      env = androidBridgeEnv // {
        KORRI_PORTAL_BUNDLE = "${packages.portal-bundle}/bin/portal-bundle";
      };
      script = ''
        ${androidSetup}
        export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
        exec bash "$KORRI_ROOT/clients/android/bridge-contract-check.sh" "$@"
      '';
    };

    korrid-check = {
      description = "Run the full host, contracts, portal, and Android check.";
      needsProseql = true;
      runtimeInputs = [ pkgs.nix ];
      env.KORRI_PORTAL_BUNDLE = "${packages.portal-bundle}/bin/portal-bundle";
      script = ''
        exec bash "$KORRI_ROOT/services/korrid/check.sh" "$@"
      '';
    };

    inputd-check = {
      description = "Run inputd Rust, portability, package, and NixOS module checks.";
      runtimeInputs = [ pkgs.nix ];
      script = ''
        exec bash "$KORRI_ROOT/services/inputd/check.sh" "$@"
      '';
    };

    korrid-config-review = {
      description = "Explain fixed local config initialization, checkpoint load, and last-known-good retention.";
      needsProseql = true;
      runtimeInputs = [
        rustToolchain
        pkgs.clang
        pkgs.llvmPackages.libclang
      ];
      env = {
        CC_x86_64_unknown_linux_gnu = "${pkgs.clang}/bin/clang";
        HOST_CC = "${pkgs.clang}/bin/clang";
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      };
      usageSuffix = " -- [storage-root]";
      script = ''
        export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
        export KORRI_CONFIG_REVIEW_IN_SHELL=1
        exec "$KORRI_ROOT/services/korrid/config-snapshot-review.sh" "$@"
      '';
    };

    korrid-plugin-review = {
      description = "Explain the enabled and disabled local announcements for a plugin.";
      needsProseql = true;
      runtimeInputs = [
        rustToolchain
        pkgs.clang
        pkgs.llvmPackages.libclang
      ];
      env = {
        CC_x86_64_unknown_linux_gnu = "${pkgs.clang}/bin/clang";
        HOST_CC = "${pkgs.clang}/bin/clang";
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      };
      usageSuffix = " -- [plugin.ts]";
      script = ''
        export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
        export KORRI_PLUGIN_REVIEW_IN_SHELL=1
        exec "$KORRI_ROOT/services/korrid/plugin-registry-review.sh" "$@"
      '';
    };

    korrid-plugin-route-review = {
      description = "Explain enabled and disabled checkpoint route resolution without Android effects.";
      needsProseql = true;
      runtimeInputs = [
        rustToolchain
        pkgs.clang
        pkgs.llvmPackages.libclang
      ];
      env = {
        CC_x86_64_unknown_linux_gnu = "${pkgs.clang}/bin/clang";
        HOST_CC = "${pkgs.clang}/bin/clang";
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      };
      usageSuffix = " -- [storage-root]";
      script = ''
        export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
        export KORRI_PLUGIN_ROUTE_REVIEW_IN_SHELL=1
        exec "$KORRI_ROOT/services/korrid/plugin-route-review.sh" "$@"
      '';
    };

    korrid-check-device = {
      description = "Run the full korrid check, then install and RPC-smoke-test it on an explicit Android target.";
      needsProseql = true;
      runtimeInputs = [
        pkgs.android-tools
        pkgs.coreutils
        pkgs.nix
      ];
      env.KORRI_PORTAL_BUNDLE = "${packages.portal-bundle}/bin/portal-bundle";
      usageSuffix = " -- <adb-serial>";
      script = ''
        serial="''${1:?usage: korrid-check-device <adb-serial>}"
        shift
        ${adbPreflight}
        export KORRI_ANDROID_DEVICE="$serial"
        exec "$KORRI_ROOT/services/korrid/check.sh" --device "$@"
      '';
    };

    android-app-route-check = {
      description = "Build Korri, then prove the installed plugin-backed Android app route through portal and PackageManager on an explicit Android target.";
      needsProseql = true;
      runtimeInputs = [
        pkgs.android-tools
        pkgs.coreutils
        pkgs.imagemagick
        pkgs.nix
        pkgs.tesseract
      ];
      env.KORRI_PORTAL_BUNDLE = "${packages.portal-bundle}/bin/portal-bundle";
      usageSuffix = " -- <adb-serial>";
      script = ''
        serial="''${1:?usage: android-app-route-check <adb-serial>}"
        shift
        ${adbPreflight}
        export KORRI_ANDROID_DEVICE="$serial"
        "$KORRI_ROOT/services/korrid/check.sh" "$@"
        exec "$KORRI_ROOT/services/korrid/android-app-route-check.sh" "$serial"
      '';
    };

    android-game-discovery-check = {
      description = "Build Korri, then prove selected-folder GBA discovery, dedupe, permission recovery, and RetroArch launch on an explicit Android target.";
      needsProseql = true;
      runtimeInputs = androidInputs ++ [
        pkgs.android-tools
        pkgs.curl
        pkgs.gnugrep
        pkgs.gnused
        pkgs.jq
        pkgs.nix
        pkgs.websocat
      ];
      env = androidEnv // {
        KORRI_PORTAL_BUNDLE = "${packages.portal-bundle}/bin/portal-bundle";
      };
      usageSuffix = " -- --serial <adb-serial>";
      script = ''
        serial=""
        args=("$@")
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --serial)
              serial="''${2:-}"
              shift 2
              ;;
            *)
              shift
              ;;
          esac
        done
        if [[ -z "$serial" ]]; then
          echo "usage: android-game-discovery-check --serial <adb-serial>" >&2
          exit 1
        fi
        ${adbPreflight}
        ${androidSetup}
        export KORRI_ANDROID_DEVICE="$serial"
        "$KORRI_ROOT/services/korrid/check.sh"
        exec "$KORRI_ROOT/services/korrid/android-game-discovery-check.sh" "''${args[@]}"
      '';
    };

    korrid-script-device = {
      description = "Run the example TypeScript plugin on an Android device.";
      needsProseql = true;
      runtimeInputs = [
        rustToolchain
        pkgs.android-tools
        pkgs.cargo-ndk
        pkgs.clang
        pkgs.coreutils
        pkgs.gnugrep
        pkgs.llvmPackages.libclang
      ];
      env = {
        ANDROID_NDK_HOME = android.ndkRoot;
        ANDROID_NDK_ROOT = android.ndkRoot;
        BINDGEN_EXTRA_CLANG_ARGS = "--target=aarch64-linux-android21 --sysroot=${android.ndkRoot}/toolchains/llvm/prebuilt/linux-x86_64/sysroot";
        CC_x86_64_unknown_linux_gnu = "${pkgs.clang}/bin/clang";
        HOST_CC = "${pkgs.clang}/bin/clang";
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      };
      usageSuffix = " -- <adb-serial>";
      script = ''
        export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
        exec "$KORRI_ROOT/services/korrid/script-device-check.sh" "$@"
      '';
    };

    brain-service-check = deviceScript "brain-service-check" "brain-service-check.sh" {
      description = "Check on a device that the embedded korrid brain survives after the portal screen leaves foreground.";
      runtimeInputs = [ pkgs.gnugrep ];
    };

    overlay-accept = {
      description = "Run the human-led, state-restoring unified Android gameplay-overlay acceptance gate. Set KORRI_OVERLAY_ACCEPT_SCOPE=full (default) or stream; stream skips the local RetroArch stages already proven by ra-accept and records that narrowed scope in its evidence.";
      usageSuffix = " -- <adb-serial> <exact-device-model> <exact-hardware-serial> <direct-launch-package> <unrelated-package> [evidence-dir]";
      runtimeInputs = [
        pkgs.android-tools
        pkgs.coreutils
        pkgs.curl
        pkgs.diffutils
        pkgs.gnugrep
        pkgs.gnused
        pkgs.gnutar
        pkgs.jq
        pkgs.python3
        pkgs.websocat
      ];
      script = ''
        serial="''${1:?usage: overlay-accept <adb-serial> <exact-device-model> <exact-hardware-serial> <direct-launch-package> <unrelated-package> [evidence-dir]}"
        : "''${2:?usage: overlay-accept <adb-serial> <exact-device-model> <exact-hardware-serial> <direct-launch-package> <unrelated-package> [evidence-dir]}" \
          "''${3:?usage: overlay-accept <adb-serial> <exact-device-model> <exact-hardware-serial> <direct-launch-package> <unrelated-package> [evidence-dir]}"
        ${adbPreflight}
        exec "$KORRI_ROOT/clients/android/overlay-acceptance.sh" "$@"
      '';
    };

    storage-notice-check = deviceScript "storage-notice-check" "storage-notice-check.sh" {
      description = "Build, install, and verify on a device that denied storage access shows a reachable portal prompt.";
      needsProseql = true;
      runtimeInputs = androidInputs ++ [
        rustToolchain
        pkgs.android-tools
        pkgs.cargo-ndk
        pkgs.clang
        pkgs.gnugrep
        pkgs.nix
        pkgs.llvmPackages.libclang
      ];
      env = androidEnv // {
        ANDROID_NDK_HOME = android.ndkRoot;
        ANDROID_NDK_ROOT = android.ndkRoot;
        BINDGEN_EXTRA_CLANG_ARGS = "--target=aarch64-linux-android21 --sysroot=${android.ndkRoot}/toolchains/llvm/prebuilt/linux-x86_64/sysroot";
        CC_x86_64_unknown_linux_gnu = "${pkgs.clang}/bin/clang";
        HOST_CC = "${pkgs.clang}/bin/clang";
        LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      };
      setup = ''
        ${androidSetup}
        export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
      '';
    };

    storage-notice-shots = deviceScript "storage-notice-shots" "storage-notice-shots.sh" {
      description = "Capture device screenshots of the portal with storage access denied and after confirming the prompt.";
      usageSuffix = " -- <adb-serial> [output-dir]";
      runtimeInputs = [ pkgs.gnugrep ];
    };

    journey-compare = deviceScript "journey-compare" "journey-compare.sh" {
      description = "Compare the Android game journey after leaving with Back versus Home, reporting resume/restart evidence.";
      runtimeInputs = [
        pkgs.gnugrep
        pkgs.gnused
      ];
    };

    journey-resume = deviceScript "journey-resume" "journey-resume.sh" {
      description = "Verify on a device that the Home/task-switch Android app journey resumes the same process on screen.";
      usageSuffix = " -- <adb-serial> [package] [tap-x tap-y]";
      runtimeInputs = [
        pkgs.gnugrep
        pkgs.gnused
        pkgs.imagemagick
        pkgs.tesseract
      ];
    };

    journey-switch = deviceScript "journey-switch" "journey-switch.sh" {
      description = "Report what appears when the user opens Korri again after switching away from an Android game with Home.";
      usageSuffix = " -- <adb-serial> [label]";
      runtimeInputs = [
        pkgs.gnugrep
        pkgs.gnused
      ];
    };

    launch-liveness-check = deviceScript "launch-liveness-check" "launch-liveness-check.sh" {
      description = "Measure whether pid, process state, and Android tasks truthfully identify an Android game launch as alive.";
      runtimeInputs = [
        pkgs.gawk
        pkgs.gnugrep
      ];
    };

    korrid-test = {
      description = "Run the korrid host test suite.";
      needsProseql = true;
      runtimeInputs = [
        rustToolchain
        pkgs.clang
        pkgs.llvmPackages.libclang
      ];
      env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
      script = ''
        export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/korrid-target"
        cd "$KORRI_ROOT/services/korrid"
        exec cargo test "$@"
      '';
    };

    portal-bundle = {
      description = "Build the portal into the Android app assets.";
      runtimeInputs = [
        pkgs.bun
        pkgs.coreutils
      ];
      script = ''
        cd "$KORRI_ROOT/surfaces/shift"
        bun install --frozen-lockfile --ignore-scripts
        cd "$KORRI_ROOT/clients/portal"
        bun install --frozen-lockfile --ignore-scripts
        bun run build
        rm -rf "$KORRI_ROOT/clients/android/app/src/main/assets/portal"
        cp -r dist "$KORRI_ROOT/clients/android/app/src/main/assets/portal"
      '';
    };

    portal-check = {
      description = "Run portal unit tests and typecheck.";
      runtimeInputs = [ pkgs.bun ];
      script = ''
        cd "$KORRI_ROOT/surfaces/shift"
        bun install --frozen-lockfile --ignore-scripts
        cd "$KORRI_ROOT/clients/portal"
        bun install --frozen-lockfile --ignore-scripts
        bun test
        bun run typecheck
      '';
    };

    shift-check = {
      description = "Run the Shift surface unit tests and typecheck.";
      runtimeInputs = [ pkgs.bun ];
      script = ''
        cd "$KORRI_ROOT/surfaces/shift"
        bun install --frozen-lockfile --ignore-scripts
        bun test
        bun run typecheck
      '';
    };

    pico-check = {
      description = "Run the Pico surface unit tests, gates, and typecheck.";
      runtimeInputs = [ pkgs.bun ];
      script = ''
        cd "$KORRI_ROOT/surfaces/pico"
        bun install --frozen-lockfile --ignore-scripts
        bun test
        bun run typecheck
      '';
    };

    portal-dev = {
      description = "Serve the portal on the local network.";
      runtimeInputs = [ pkgs.bun ];
      script = ''
        cd "$KORRI_ROOT/clients/portal"
        exec bun run dev "$@"
      '';
    };

    ra-accept = {
      description = "Run read-only-prerequisite RetroArch acceptance on Android.";
      runtimeInputs = [
        pkgs.android-tools
        pkgs.coreutils
        pkgs.curl
        pkgs.diffutils
        pkgs.gnugrep
        pkgs.gnused
        pkgs.imagemagick
        pkgs.jq
        pkgs.tesseract
        pkgs.websocat
      ];
      usageSuffix = " -- <adb-serial> <exact-device-model> <exact-hardware-serial>";
      script = ''
        if [[ "$#" -ne 3 ]]; then
          echo "usage: ra-accept <adb-serial> <exact-device-model> <exact-hardware-serial>" >&2
          exit 2
        fi
        exec ${pkgs.bash}/bin/bash "$KORRI_ROOT/plugins/retroarch/android/device-acceptance.sh" "$@"
      '';
    };

    ra-build = {
      description = "Build and validate the patched arm64 RetroArch runtime.";
      runtimeInputs = retroarchInputs;
      env = retroarchEnv;
      script = ''
        ${retroarchSetup}
        exec "$KORRI_ROOT/plugins/retroarch/android/build.sh" "$@"
      '';
    };

    ra-check = {
      description = "Test the RetroArch lifecycle and build a fresh APK.";
      runtimeInputs = [
        pkgs.coreutils
        pkgs.git
        pkgs.gnugrep
        pkgs.gnused
      ];
      script = ''
        "$KORRI_ROOT/plugins/retroarch/android/test-fetch-upstream.sh"
        "$KORRI_ROOT/plugins/retroarch/android/test-build.sh"
        "$KORRI_ROOT/plugins/retroarch/android/test-install-device.sh"
        "$KORRI_ROOT/plugins/retroarch/android/test-acceptance-contract.sh"
        "$KORRI_ROOT/plugins/retroarch/android/test-distribution.sh"
        "$KORRI_ROOT/plugins/retroarch/android/test-distribution-workflow.sh"
        exec ${packages.ra-build}/bin/ra-build
      '';
    };

    ra-dist = {
      description = "Build, verify, and stage the custom RetroArch distribution candidate.";
      runtimeInputs = [ pkgs.coreutils ];
      usageSuffix = " -- <output-directory>";
      script = ''
        output_dir="''${1:?usage: ra-dist <output-directory>}"
        shift
        ${packages.ra-check}/bin/ra-check
        exec "$KORRI_ROOT/plugins/retroarch/android/stage-distribution.sh" "$output_dir" "$@"
      '';
    };

    ra-sign = {
      description = "Sign and verify a staged RetroArch distribution candidate.";
      runtimeInputs = retroarchSigningInputs;
      env = retroarchEnv;
      usageSuffix = " -- <candidate-apk> <output-directory>";
      script = ''
        candidate="''${1:?usage: ra-sign <candidate-apk> <output-directory>}"
        output_dir="''${2:?usage: ra-sign <candidate-apk> <output-directory>}"
        shift 2
        ${retroarchSetup}
        exec "$KORRI_ROOT/plugins/retroarch/android/sign-distribution.sh" "$candidate" "$output_dir" "$@"
      '';
    };

    mgba-build = {
      description = "Build the @korri:mgba arm64 libretro core and stage its packaging bridge.";
      runtimeInputs = retroarchInputs;
      env = retroarchEnv;
      script = ''
        ${packages.ra-fetch}/bin/ra-fetch
        ${retroarchSetup}
        exec "$KORRI_ROOT/plugins/mgba/android/build.sh" "$@"
      '';
    };

    ra-deploy = {
      description = "Build, validate, and install the RetroArch fork on Android.";
      runtimeInputs = retroarchInputs ++ [ pkgs.android-tools ];
      env = retroarchEnv;
      usageSuffix = " -- <adb-serial>";
      script = ''
        serial="''${1:?usage: ra-deploy <adb-serial>}"
        shift
        ${adbPreflight}
        ${packages.ra-build}/bin/ra-build
        ${retroarchSetup}
        exec "$KORRI_ROOT/plugins/retroarch/android/install-device.sh" "$serial" "$@"
      '';
    };

    ra-fetch = {
      description = "Recreate the pinned, patched RetroArch source tree.";
      runtimeInputs = [
        pkgs.coreutils
        pkgs.git
      ];
      script = ''
        exec "$KORRI_ROOT/plugins/retroarch/android/fetch-upstream.sh" "$@"
      '';
    };
  };

  exports = pkgs.lib.mapAttrs (
    _: task:
    pkgs.lib.concatStringsSep "\n" (
      pkgs.lib.mapAttrsToList (name: value: ''export ${name}="${value}"'') (task.env or { })
    )
  ) definitions;

  makeTask =
    name: task:
    pkgs.writeShellApplication {
      inherit name;
      runtimeInputs = [
        pkgs.bash
        pkgs.coreutils
        pkgs.git
      ]
      ++ task.runtimeInputs;
      text = ''
        if [[ -n "''${KORRI_ROOT:-}" ]]; then
          korri_root="$KORRI_ROOT"
        else
          korri_root="$PWD"
          while [[ "$korri_root" != / && ! -f "$korri_root/nix/tasks.nix" ]]; do
            korri_root="$(dirname "$korri_root")"
          done
        fi
        if [[ ! -f "$korri_root/flake.nix" || ! -f "$korri_root/nix/tasks.nix" ]]; then
          echo "Korri checkout not found; run inside it or set KORRI_ROOT" >&2
          exit 1
        fi
        KORRI_ROOT="$(cd "$korri_root" && pwd -P)"
        export KORRI_ROOT
        ${pkgs.lib.optionalString (task.needsProseql or false) proseqlSource.hydrateShell}
        ${exports.${name}}
        ${task.script}
      '';
    };

  packages = pkgs.lib.mapAttrs makeTask definitions;

  helpText =
    pkgs.lib.concatStringsSep "\n" (
      pkgs.lib.mapAttrsToList (
        name: task: "  nix run .#${name}${task.usageSuffix or ""}\n      ${task.description}"
      ) definitions
    )
    + extraHelpText;

  help = pkgs.writeShellApplication {
    name = "help";
    runtimeInputs = [ ];
    text = ''
      cat <<'EOF'
      Korri tasks (declared in nix/tasks.nix):

      ${helpText}
      EOF
    '';
  };

  toApp = package: {
    type = "app";
    program = "${package}/bin/${package.name}";
  };
in
(pkgs.lib.mapAttrs (_: package: toApp package) packages)
// {
  help = toApp help;
  default = toApp help;
}

{
  pkgs,
  system,
  inputplumberNixpkgs,
  crane,
  korriBundleModule,
  korriInputModule,
  korridLinuxDeviceModule,
  korriLinuxHostModule,
  korridPackage,
  sunshinePackage,
  sunshineV4l2m2mPackage ? null,
}:

let
  inputplumberPkgs = import inputplumberNixpkgs { inherit system; };
  inputplumberRuntime = pkgs.callPackage ./inputplumber-package.nix {
    inputplumber = inputplumberPkgs.inputplumber;
  };
  inputplumberData = import ./inputplumber-data.nix { inherit pkgs; };
  inputplumberKorri = inputplumberData.compose { inherit inputplumberRuntime; };
  retroarchInputplumberAutoconfig = pkgs.callPackage ./retroarch-inputplumber-autoconfig.nix { };
  sunshineApprovedPatches = import ../../sunshine/approved-patches.nix;
  sunshinePatchPaths = map (record: record.path) sunshineApprovedPatches.patches;
  sunshinePatchManifest =
    builtins.concatStringsSep "\n" (
      map (record: "patch=${record.name} sha256=${record.sha256}") sunshineApprovedPatches.patches
    )
    + "\n";
  sunshinePatchManifestFile = pkgs.writeText "sunshine-korri-approved-patch-manifest" sunshinePatchManifest;
  sunshineApprovedBaseDerivations =
    sunshineApprovedPatches.approvedBaseDerivationsByProfile.${sunshinePackage.korriBuildProfile}
      or [ ];
  sunshineBasePackage = pkgs.sunshine.override {
    cudaSupport = sunshinePackage.korriCudaEnabled;
  };
  sunshineApprovedDeviceBaseDerivation =
    sunshineApprovedPatches.approvedDeviceBaseDerivations.${sunshinePackage.korriBuildProfile} or null;
  androidMoonlightRoot = ../../../clients/android/app/src/main/jni/moonlight-core/moonlight-common-c/src;
  inputdPackage = import ../package.nix { inherit pkgs crane; };
  devApp = import ./dev-app.nix {
    inherit pkgs inputdPackage korridPackage;
  };
  korriBundle = import ./korri-bundle.nix {
    inherit
      pkgs
      inputdPackage
      inputplumberKorri
      korridPackage
      ;
  };
  toApp = package: {
    type = "app";
    program = "${package}/bin/${package.name}";
  };
in
{
  apps = {
    korri-dev = toApp devApp;
    korri-bundle-select = {
      type = "app";
      program = "${inputdPackage}/bin/korri-bundle-select";
    };
  };
  packages = {
    inputplumber-korri = inputplumberKorri;
    retroarch-inputplumber-autoconfig = retroarchInputplumberAutoconfig;
    korri-inputd = inputdPackage;
    korri-bundle = korriBundle;
    sunshine-korri = sunshinePackage;
  }
  // pkgs.lib.optionalAttrs (sunshineV4l2m2mPackage != null) {
    sunshine-korri-v4l2m2m = sunshineV4l2m2mPackage;
    sunshine-v4l2m2m-probe = import ../../sunshine/v4l2m2m-probe.nix {
      inherit pkgs;
      ffmpeg = pkgs.callPackage ../../sunshine/ffmpeg-v4l2m2m-static.nix { };
    };
  };
  checks = {
    sunshine-korri-package = pkgs.runCommand "sunshine-korri-package-check" { } ''
      test -f ${sunshinePackage}/bin/sunshine
      test -x ${sunshinePackage}/bin/sunshine
      ${
        if sunshinePackage.korriCudaEnabled then
          ''
            test ! -L ${sunshinePackage}/bin/sunshine
            test -L ${sunshinePackage}/bin/.sunshine-wrapped
            grep -F '"${sunshinePackage}/bin/.sunshine-wrapped"' ${sunshinePackage}/bin/sunshine >/dev/null
          ''
        else
          ''
            test -L ${sunshinePackage}/bin/sunshine
            case "$(readlink -f ${sunshinePackage}/bin/sunshine)" in
              ${sunshinePackage}/bin/sunshine-*) ;;
              *) exit 1 ;;
            esac
          ''
      }
      test "${sunshinePackage.pname}" = sunshine-korri
      test "${sunshinePackage.version}" = "${pkgs.sunshine.version}-korri"
      test "${toString (builtins.length sunshinePackage.korriPatchNames)}" = 16
      test "${sunshinePackage.korriBaseSunshineVersion}" = "${sunshineApprovedPatches.baseSunshineVersion}"
      test "${sunshinePackage.korriApprovedBaseSunshineSourceHash}" = "${sunshineApprovedPatches.approvedBaseSourceHash}"
      test "${pkgs.sunshine.src.outputHash}" = "${sunshineApprovedPatches.approvedBaseSourceHash}"
      test "${sunshinePackage.korriBaseSunshineSource}" = "${builtins.unsafeDiscardStringContext (toString pkgs.sunshine.src)}"
      test "${sunshinePackage.korriBuildProfile}" = "${system}-${
        if sunshinePackage.korriCudaEnabled then "cuda" else "software"
      }"
      test "${sunshinePackage.korriBaseSunshineDerivation}" = "${builtins.unsafeDiscardStringContext sunshineBasePackage.drvPath}"
      test "${toString (builtins.elem sunshinePackage.korriBaseSunshineDerivation sunshineApprovedBaseDerivations)}" = 1
      test "${sunshinePackage.korriApprovedBaseSunshineDerivation}" = "${sunshinePackage.korriBaseSunshineDerivation}"
      test "${toString (sunshineApprovedDeviceBaseDerivation != null)}" = 1
      test "${sunshinePackage.korriReviewedLibavcodecVersion}" = "${sunshineApprovedPatches.reviewedLibavcodecVersion}"
      test "${sunshinePackage.korriReviewedFfmpegCommit}" = "${sunshineApprovedPatches.reviewedFfmpegCommit}"
      test "${sunshinePackage.korriReviewedFfmpegSourceHash}" = "${sunshineApprovedPatches.reviewedFfmpegSourceHash}"
      test "${toString sunshinePackage.korriReviewedNvencApiMajor}" = "${toString sunshineApprovedPatches.reviewedNvencApiMajor}"
      test "${toString sunshinePackage.korriReviewedNvencApiMinor}" = "${toString sunshineApprovedPatches.reviewedNvencApiMinor}"
      test "${toString sunshinePackage.korriCudaEnabled}" = "${toString (system == "x86_64-linux")}"
      test "${sunshinePackage.korriPatchSetSha256}" = "${sunshineApprovedPatches.patchSetSha256}"
      provenance=${sunshinePackage}/${sunshinePackage.korriProvenanceRelativePath}
      test -f "$provenance"
      grep -Fx 'package=sunshine-korri' "$provenance" >/dev/null
      grep -Fx 'build_profile=${sunshinePackage.korriBuildProfile}' "$provenance" >/dev/null
      grep -Fx 'cuda_enabled=${
        if sunshinePackage.korriCudaEnabled then "1" else "0"
      }' "$provenance" >/dev/null
      grep -Fx 'approved_base_sunshine_source_hash=${sunshineApprovedPatches.approvedBaseSourceHash}' "$provenance" >/dev/null
      grep -Fx 'approved_base_sunshine_derivation=${sunshinePackage.korriBaseSunshineDerivation}' "$provenance" >/dev/null
      grep -Fx 'reviewed_ffmpeg_commit=${sunshineApprovedPatches.reviewedFfmpegCommit}' "$provenance" >/dev/null
      grep -Fx 'reviewed_ffmpeg_source_hash=${sunshineApprovedPatches.reviewedFfmpegSourceHash}' "$provenance" >/dev/null
      grep -Fx 'reviewed_nvenc_api=${toString sunshineApprovedPatches.reviewedNvencApiMajor}.${toString sunshineApprovedPatches.reviewedNvencApiMinor}' "$provenance" >/dev/null
      grep -Fx 'executable=bin/sunshine' "$provenance" >/dev/null
      grep -Fx 'patch_set_sha256=${sunshinePackage.korriPatchSetSha256}' "$provenance" >/dev/null
      touch "$out"
    '';
    sunshine-korri-runtime-settings = import ../../sunshine/runtime-settings-check.nix {
      inherit pkgs sunshinePackage;
      approvedPatchesPath = ../../sunshine/approved-patches.nix;
      patchPaths = sunshinePatchPaths;
      packagePath = ../../sunshine/package.nix;
      readmePath = ../../sunshine/README.md;
    };
    sunshine-korri-input-seat-patch = import ../../sunshine/input-seat-patch-check.nix {
      inherit pkgs sunshinePackage;
      approvedPatchesPath = ../../sunshine/approved-patches.nix;
      nonblockingTestPath = ../../sunshine/test-nonblocking-mirror.py;
      patchPath = ../../sunshine/patches/0015-add-korri-input-seat-event-mirror.patch;
      packagePath = ../../sunshine/package.nix;
      readmePath = ../../sunshine/README.md;
    };
    sunshine-korri-certificate-control = import ../../sunshine/certificate-control-check.nix {
      inherit pkgs sunshinePackage;
      approvedPatchesPath = ../../sunshine/approved-patches.nix;
      patchPath = ../../sunshine/patches/0020-add-korrid-certificate-control.patch;
      packagePath = ../../sunshine/package.nix;
      testPath = ../../sunshine/test-certificate-control.cpp;
    };
    sunshine-korri-v4l2m2m = import ../../sunshine/v4l2m2m-check.nix {
      inherit pkgs sunshinePackage sunshineV4l2m2mPackage;
      approvedPatchesPath = ../../sunshine/approved-patches.nix;
      patchPath = ../../sunshine/patches/0021-add-v4l2m2m-encoder.patch;
      ffmpegPatchPath = ../../sunshine/patches/ffmpeg/0001-fix-v4l2m2m-buffer-alignment.patch;
      ffmpegPackagePath = ../../sunshine/ffmpeg-v4l2m2m-static.nix;
      packagePath = ../../sunshine/package.nix;
      readmePath = ../../sunshine/README.md;
    };
    sunshine-korri-android-client-protocol = import ../../sunshine/android-client-protocol-check.nix {
      inherit pkgs;
      sunshinePatchPath = ../../sunshine/patches/0001-add-runtime-settings-protocol-surface.patch;
      clientHeaderPath = androidMoonlightRoot + "/Limelight.h";
      clientInternalHeaderPath = androidMoonlightRoot + "/SunshineRuntimeSettings.h";
      clientProtocolPath = androidMoonlightRoot + "/SunshineRuntimeSettings.c";
      clientDispatchPath = androidMoonlightRoot + "/SunshineRuntimeSettingsDispatch.c";
      clientControlStreamPath = androidMoonlightRoot + "/ControlStream.c";
      clientConnectionPath = androidMoonlightRoot + "/Connection.c";
      clientJniPath = ../../../clients/android/app/src/main/jni/moonlight-core/simplejni.c;
      clientJavaPath = ../../../clients/android/app/src/main/java/com/limelight/nvstream/jni/MoonBridge.java;
      clientSnapshotJavaPath = ../../../clients/android/app/src/main/java/com/limelight/nvstream/jni/SunshineRuntimeSettingsSnapshot.java;
      clientControllerHandlerPath = ../../../clients/android/app/src/main/java/com/limelight/binding/input/ControllerHandler.java;
      clientControllerHeartbeatPath = ../../../clients/android/app/src/main/java/com/limelight/binding/input/ControllerHeartbeat.java;
      clientControllerHeartbeatTestPath = ../../../clients/android/app/src/test/java/com/limelight/binding/input/ControllerHeartbeatTest.java;
      nativeTestPath = ../../../clients/android/app/src/test-native/sunshine-runtime-settings-test.c;
    };
    inputplumber-korri-package = import ./inputplumber-package-check.nix {
      inherit
        pkgs
        inputplumberRuntime
        inputplumberKorri
        retroarchInputplumberAutoconfig
        ;
    };
    korri-input-seat-receiver = import ./korri-input-seat-receiver-check.nix {
      inherit pkgs inputdPackage;
    };
    korri-inputd-package = pkgs.runCommand "korri-inputd-package-check" { } ''
      test -x ${inputdPackage}/bin/korri-inputd
      test -x ${inputdPackage}/bin/korri-input-seat-receiver
      test -x ${inputdPackage}/bin/korri-bundle-launch
      test -x ${inputdPackage}/bin/korri-bundle-select
      test -x ${inputdPackage}/bin/korri-device-gate
      test "$(sha256sum ${inputdPackage}/bin/korri-device-gate | cut -d' ' -f1)" = \
        "$(sha256sum ${../deploy/device-check.sh} | cut -d' ' -f1)"
      gate=${inputdPackage}/bin/korri-device-gate
      grep -Fx "EXPECTED_SUNSHINE_FORMAT='1'" "$gate" >/dev/null
      grep -Fx "EXPECTED_SUNSHINE_BASE_VERSION='${sunshineApprovedPatches.baseSunshineVersion}'" "$gate" >/dev/null
      grep -Fx "EXPECTED_SUNSHINE_BASE_SOURCE_HASH='${sunshineApprovedPatches.approvedBaseSourceHash}'" "$gate" >/dev/null
      grep -Fx "EXPECTED_SUNSHINE_BASE_DERIVATION='${sunshineApprovedPatches.approvedDeviceBaseDerivation}'" "$gate" >/dev/null
      grep -Fx "EXPECTED_SUNSHINE_LIBAVCODEC_VERSION='${sunshineApprovedPatches.reviewedLibavcodecVersion}'" "$gate" >/dev/null
      grep -Fx "EXPECTED_SUNSHINE_FFMPEG_COMMIT='${sunshineApprovedPatches.reviewedFfmpegCommit}'" "$gate" >/dev/null
      grep -Fx "EXPECTED_SUNSHINE_FFMPEG_SOURCE_HASH='${sunshineApprovedPatches.reviewedFfmpegSourceHash}'" "$gate" >/dev/null
      grep -Fx "EXPECTED_SUNSHINE_NVENC_API='${toString sunshineApprovedPatches.reviewedNvencApiMajor}.${toString sunshineApprovedPatches.reviewedNvencApiMinor}'" "$gate" >/dev/null
      grep -Fx "EXPECTED_SUNSHINE_PATCH_SET_SHA256='${sunshineApprovedPatches.patchSetSha256}'" "$gate" >/dev/null
      grep '^patch=' "$gate" > actual-patch-manifest
      cmp actual-patch-manifest ${sunshinePatchManifestFile}

      # Linux resolves /proc/PID/exe through the bin/sunshine symlink. Exercise
      # the gate's exact resolver against the shipped versioned target.
      declared=${sunshinePackage}/bin/sunshine
      test -f "$declared"
      ${
        if sunshinePackage.korriCudaEnabled then
          ''
            wrapped=${sunshinePackage}/bin/.sunshine-wrapped
            test ! -L "$declared"
            test -L "$wrapped"
            grep -F 'bin/.sunshine-wrapped' "$declared" >/dev/null
            running="$(readlink -f -- "$wrapped")"
          ''
        else
          ''
            test -L "$declared"
            running="$(readlink -f -- "$declared")"
          ''
      }
      case "$running" in
        ${sunshinePackage}/bin/sunshine-*) ;;
        *) echo "Sunshine did not resolve to its versioned package target" >&2; exit 1 ;;
      esac
      sed -n '/^REMOTE_SUNSHINE_PACKAGE_ROOT=/,/^}/p' "$gate" > sunshine-executable-resolver.sh
      # shellcheck disable=SC1091
      source ./sunshine-executable-resolver.sh
      remote_resolve_sunshine_executable "$running" "$declared"
      test "$REMOTE_SUNSHINE_PACKAGE_ROOT" = ${sunshinePackage}

      test -x ${inputdPackage}/bin/korri-sunshine-state-digest
      test -x ${inputdPackage}/bin/korri-ledger-proof
      test -x ${inputdPackage}/bin/korri-virtual-target-acl
      test -x ${devApp}/bin/korri-dev
      grep -F 'KORRI_INPUTD_PROFILE=development' ${devApp}/bin/korri-dev >/dev/null
      grep -F 'KORRI_INPUTD_SOURCE="$physical_input"' ${devApp}/bin/korri-dev >/dev/null
      test "$(readlink -f ${korriBundle}/bin/inputplumber)" = ${inputplumberKorri}/bin/inputplumber
      test "$(readlink -f ${korriBundle}/bin/korri-inputd)" = ${inputdPackage}/bin/korri-inputd
      test "$(readlink -f ${korriBundle}/bin/korrid)" = ${korridPackage}/bin/korrid
      test "$(readlink -f ${korriBundle}/share/inputplumber)" = ${inputplumberKorri}/share/inputplumber
      test "$(readlink -f ${korriBundle}/share/korri-input-profile)" = \
        ${inputplumberKorri}/share/inputplumber/profiles/${inputplumberData.resolvedProfile}
      touch "$out"
    '';
    korri-input-module = import ./korri-input-module-check.nix {
      module = korriInputModule;
      bundleModule = korriBundleModule;
      inherit pkgs inputdPackage inputplumberKorri;
    };
    korri-bundle-module = import ./korri-bundle-module-check.nix {
      inherit
        pkgs
        korriBundleModule
        korriInputModule
        korridLinuxDeviceModule
        inputdPackage
        inputplumberKorri
        korridPackage
        korriBundle
        ;
    };
    korrid-linux-device-module = import ../../korrid/nixos-module-check.nix {
      module = korridLinuxDeviceModule;
      bundleModule = korriBundleModule;
      inherit
        pkgs
        korridPackage
        inputdPackage
        korriBundle
        ;
    };
    korri-linux-host-module = import ./korri-linux-host-module-check.nix {
      module = korriLinuxHostModule;
      inherit
        pkgs
        sunshinePackage
        sunshineV4l2m2mPackage
        inputdPackage
        inputplumberKorri
        korridPackage
        korriBundle
        ;
    };
  };
}

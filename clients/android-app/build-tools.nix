# Android's build tools alone (apksigner, aapt2), at the version that
# ../android/sdk.nix builds with, for the release signing step
# (sign-release.sh, #434 D12). No NDK, platform or emulator: the step runs on
# the owner's machine, where the key is, and only signs.
{ pkgs }:
let
  sdk = import ../android/sdk.nix { inherit pkgs; };
  composition = pkgs.androidenv.composeAndroidPackages {
    buildToolsVersions = [ sdk.buildToolsVersion ];
    platformVersions = [ ];
    includeEmulator = false;
    includeSystemImages = false;
    includeNDK = false;
    includeCmake = false;
    includeSources = false;
    extraLicenses = [ "android-sdk-license" ];
  };
in
"${composition.androidsdk}/libexec/android-sdk/build-tools/${sdk.buildToolsVersion}"

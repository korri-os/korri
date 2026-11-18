# Android's build tools (apksigner, aapt2), at the version that
# nix/android-sdk.nix builds with, and its platform tools (adb), for the steps
# that run on the owner's machine: the release signing step (sign-release.sh)
# and D12's owner binding (d12-binding.sh), #434. No NDK, platform or
# emulator: those steps only sign, read manifests and talk to a phone.
{ pkgs }:
let
  sdk = import ../../nix/android-sdk.nix { inherit pkgs; };
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
  root = "${composition.androidsdk}/libexec/android-sdk";
in
{
  buildTools = "${root}/build-tools/${sdk.buildToolsVersion}";
  platformTools = "${root}/platform-tools";
}

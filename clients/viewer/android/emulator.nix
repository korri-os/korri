# The emulator for the app's proof (proof/run.py): Android 12 (API 31,
# ADR 0064), google_apis, x86_64, as the D5 spike ran it. A test tool; the
# APK build (../android.nix) does not use it.
{ pkgs }:
let
  sdk = import ../../android/sdk.nix { inherit pkgs; };
  composition = pkgs.androidenv.composeAndroidPackages {
    buildToolsVersions = [ sdk.buildToolsVersion ];
    platformVersions = [ "31" ];
    includeEmulator = true;
    includeSystemImages = true;
    systemImageTypes = [ "google_apis" ];
    abiVersions = [ "x86_64" ];
    includeNDK = false;
    includeSources = false;
    extraLicenses = [
      "android-sdk-license"
      "android-sdk-preview-license"
      "google-gdk-license"
      "intel-android-extra-license"
      "intel-android-sysimage-license"
    ];
  };
in
{
  androidSdk = composition.androidsdk;
  sdkRoot = "${composition.androidsdk}/libexec/android-sdk";
  systemImage = "system-images;android-31;google_apis;x86_64";
  inherit (sdk) buildToolsVersion;
}

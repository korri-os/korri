# The Android SDK and NDK that the Korri app (clients/android-app) and the
# viewer's Android parts (clients/viewer/android.nix) build with. No Gradle.
{ pkgs }:
let
  buildToolsVersion = "35.0.0";
  ndkVersion = "27.0.12077973";

  androidComposition = pkgs.androidenv.composeAndroidPackages {
    buildToolsVersions = [ buildToolsVersion ];
    platformVersions = [ "36" ];
    ndkVersions = [ ndkVersion ];
    includeSources = false;
    includeNDK = true;
    # The viewer's C parts use nixpkgs' CMake with the NDK's toolchain file.
    includeCmake = false;
    extraLicenses = [
      "android-googletv-license"
      "android-sdk-arm-dbt-license"
      "android-sdk-license"
      "android-sdk-preview-license"
      "google-gdk-license"
      "intel-android-extra-license"
      "intel-android-sysimage-license"
      "mips-android-sysimage-license"
    ];
  };
in
rec {
  inherit buildToolsVersion ndkVersion;
  androidSdk = androidComposition.androidsdk;
  sdkRoot = "${androidSdk}/libexec/android-sdk";
  ndkRoot = "${sdkRoot}/ndk-bundle";
  jdk = pkgs.jdk17;
}

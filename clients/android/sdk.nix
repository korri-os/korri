# Android SDK/NDK composition shared by devshells and Nix tasks.
{
  pkgs,
  bridgeEmulatorProfile ? false,
}:
let
  buildToolsVersion = "35.0.0";
  ndkVersion = "27.0.12077973";
  bridgeEmulatorPlatformVersion = "34";
  bridgeEmulatorSystemImageType = "google_apis";
  bridgeEmulatorAbiVersion = "x86_64";

  androidComposition = pkgs.androidenv.composeAndroidPackages (
    {
      buildToolsVersions = [
        buildToolsVersion
        "34.0.0"
      ];
      platformVersions =
        if bridgeEmulatorProfile then [ bridgeEmulatorPlatformVersion ] else [ "36" "34" ];
      includeEmulator = bridgeEmulatorProfile;
      includeSystemImages = bridgeEmulatorProfile;
      ndkVersions = [ ndkVersion ];
      includeSources = false;
      includeNDK = true;
      cmakeVersions = [ "3.22.1" ];
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
    }
    // pkgs.lib.optionalAttrs bridgeEmulatorProfile {
      systemImageTypes = [ bridgeEmulatorSystemImageType ];
      abiVersions = [ bridgeEmulatorAbiVersion ];
    }
  );
in
rec {
  inherit
    buildToolsVersion
    ndkVersion
    bridgeEmulatorPlatformVersion
    bridgeEmulatorSystemImageType
    bridgeEmulatorAbiVersion
    ;
  bridgeEmulatorAvdPackage = "system-images;android-${bridgeEmulatorPlatformVersion};${bridgeEmulatorSystemImageType};${bridgeEmulatorAbiVersion}";
  androidSdk = androidComposition.androidsdk;
  sdkRoot = "${androidSdk}/libexec/android-sdk";
  # cargo-ndk needs only the NDK. Gradle uses the writable SDK farm created by
  # nix/android-sdk-env.sh because AGP expects sdk/ndk/<version>.
  ndkRoot = "${sdkRoot}/ndk-bundle";
  jdk = pkgs.jdk17;
  gradleOpts = "-Dorg.gradle.daemon=false -Dorg.gradle.project.android.aapt2FromMavenOverride=${sdkRoot}/build-tools/${buildToolsVersion}/aapt2";
}

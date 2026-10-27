# The restored Artemis toolchain. Keep Android composition next to the client.
{ pkgs, korri, portalBundle }:
let
  android = import ./sdk.nix { inherit pkgs; };
  rust = pkgs.rust-bin.stable.latest.default.override {
    targets = [ "aarch64-linux-android" ];
  };
  inputs = [
    android.jdk
    android.androidSdk
    rust
    pkgs.stdenv.cc
    pkgs.bun
    pkgs.gnumake
    pkgs.file
    pkgs.unzip
    pkgs.which
    pkgs.llvmPackages.libclang
  ];
  env = {
    KORRI_PORTAL_BUNDLE = portalBundle;
    KORRI_ANDROID_RELAYS_FILE = import ./product.nix { inherit pkgs korri; };
    JAVA_HOME = "${android.jdk}";
    GRADLE_OPTS = android.gradleOpts;
    KORRI_ANDROID_SDK_NAME = "android";
    KORRI_NIX_SDK = android.sdkRoot;
    KORRI_NDK_VERSION = android.ndkVersion;
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  };
  task = description: mode: {
    inherit description env;
    needsProseql = true;
    runtimeInputs = inputs;
    script = ''
      exec bash "$KORRI_ROOT/clients/android/build.sh" ${mode} "$@"
    '';
  };
in
{
  android-apk = task "Build the restored Artemis arm64 debug APK, including korrid and the portal." "apk";
  android-jvm-check = task "Run the restored Android JVM/Robolectric tests." "test";
  android-brain-check = task "Cross-check korrid's library for arm64 Android." "brain-check";
  android-device-test-apk = task "Build the real WebView/JNI/RPC device test APK." "device-test-apk";
}

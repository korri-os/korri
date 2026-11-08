# Reuse the spike's pinned prebuilt SDK, patched ss4s and video-only CLI.
{ pkgs }:
let
  spike = import ../../webos-embedded-viewer-spike { inherit pkgs; };
  nativeCheck = spike.nativeTests.overrideAttrs (old: {
    configurePhase = old.configurePhase + ''
      cmake -S . -B build -DKORRI_WEBOS_LAUNCHER=${./launcher.c}
    '';
    installPhase = ''
      export APPID=org.korri.shell
      export LD_LIBRARY_PATH="$PWD/build"
      export MEDIA_TEST_BINARY="$PWD/build/korri-webos"
      export MEDIA_TEST_MULTICALL=1
      export MEDIA_TEST_STREAM=${spike.stream}/test-1080p30.h264
      python3 -m unittest media_test_test.py
      mkdir "$out"
      touch "$out/passed"
    '';
  });
in
pkgs.stdenvNoCC.mkDerivation {
  name = "korri-webos-launcher";
  src = ../../webos-embedded-viewer-spike;
  nativeBuildInputs = [
    pkgs.cmake
    pkgs.ninja
  ];
  dontUseCmakeConfigure = true;
  dontFixup = true;
  configurePhase = ''
    cp -r ${spike.ss4s} ss4s-source
    chmod -R u+w ss4s-source
    patch -d ss4s-source -p1 < ss4s-media-test.patch
    sysroot=${spike.sdk}/arm-webos-linux-gnueabi/sysroot
    cmake -S . -B build -G Ninja \
      -DCMAKE_SYSTEM_NAME=Linux -DCMAKE_SYSTEM_PROCESSOR=armv7 \
      -DCMAKE_C_COMPILER=${spike.sdk}/bin/arm-webos-linux-gnueabi-gcc \
      -DCMAKE_CXX_COMPILER=${spike.sdk}/bin/arm-webos-linux-gnueabi-g++ \
      -DCMAKE_SYSROOT="$sysroot" -DCMAKE_FIND_ROOT_PATH="$sysroot" \
      -DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER \
      -DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY \
      -DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY \
      -DCMAKE_C_FLAGS=-mfloat-abi=softfp -DCMAKE_CXX_FLAGS=-mfloat-abi=softfp \
      -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$out" \
      -DSS4S_SOURCE="$PWD/ss4s-source" \
      -DKORRI_WEBOS_LAUNCHER=${./launcher.c}
  '';
  buildPhase = ''
    cmake --build build --parallel 2
  '';
  installPhase = ''
    test -e ${nativeCheck}/passed
    cmake --install build
    # The product has one media-test entry point, not the spike's executable.
    rm "$out/bin/media-test"
    mkdir -p "$out/share/korri-webos/licenses"
    cp ${spike.ss4s}/LICENSE "$out/share/korri-webos/licenses/ss4s.txt"
    cp ss4s-media-test.patch "$out/share/korri-webos/"
  '';
}

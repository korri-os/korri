# Workstation builds only; no TV/host contact and no launcher/packaging changes.
{ pkgs }:
let
  ss4s = pkgs.fetchzip {
    url = "https://github.com/mariotaku/ss4s/archive/9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2.tar.gz";
    sha256 = "0d9hf0d7li713mnc27g1qrjzcmb5j9m8mxxy7y7r4an91mh24lxy";
  };
  mbedtls = pkgs.fetchurl {
    url = "https://github.com/Mbed-TLS/mbedtls/releases/download/mbedtls-3.6.7/mbedtls-3.6.7.tar.bz2";
    hash = "sha256-p+i8vsDm92G0ryTyVndiazX3YvaO73nAhnejYyEtEfY=";
  };
  sdk = import ./sdk.nix { inherit pkgs; };
  build =
    host:
    pkgs.stdenvNoCC.mkDerivation {
      pname = "korri-webos-viewer-${if host then "native" else "armv7"}";
      version = "0";
      src = ./.;
      nativeBuildInputs = [
        pkgs.cmake
        pkgs.ninja
        pkgs.python3
        pkgs.binutils
      ]
      ++ pkgs.lib.optionals host [
        pkgs.stdenv.cc
        pkgs.openssl
      ];
      dontUseCmakeConfigure = true;
      dontFixup = true;
      configurePhase = ''
        cp -r ${ss4s} ss4s-source
        chmod -R u+w ss4s-source
        ${pkgs.lib.optionalString host "patch -d ss4s-source -p1 < ss4s-close-test.patch"}
        patch -d ss4s-source -p1 < ss4s.patch
        mkdir mbedtls-source expat-source opus-source
        tar xf ${mbedtls} --strip-components=1 -C mbedtls-source
        tar xf ${pkgs.expat.src} --strip-components=1 -C expat-source
        tar xf ${pkgs.libopus.src} --strip-components=1 -C opus-source
        cmake -S . -B build -G Ninja \
          -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$out" \
          -DKORRI_WEBOS_LAUNCHER=${../shell/webos/launcher.c} \
          -DKORRI_MEDIA_SOURCE=${../webos-embedded-viewer-spike/media-test.c} \
          -DSS4S_SOURCE="$PWD/ss4s-source" \
          -DCOMMON_SOURCE=${../android/app/src/main/jni/moonlight-core/moonlight-common-c} \
          -DMBEDTLS_SOURCE="$PWD/mbedtls-source" \
          -DEXPAT_SOURCE="$PWD/expat-source" -DOPUS_SOURCE="$PWD/opus-source" \
          ${
            if host then
              "-DKV_HOST_TEST=ON"
            else
              ''
                -DCMAKE_SYSTEM_NAME=Linux -DCMAKE_SYSTEM_PROCESSOR=armv7 \
                -DCMAKE_C_COMPILER=${sdk}/bin/arm-webos-linux-gnueabi-gcc \
                -DCMAKE_SYSROOT=${sdk}/arm-webos-linux-gnueabi/sysroot \
                -DCMAKE_FIND_ROOT_PATH=${sdk}/arm-webos-linux-gnueabi/sysroot \
                -DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER \
                -DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY \
                -DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY \
                -DCMAKE_C_FLAGS="-mfloat-abi=softfp -DSYS_getrandom=384"
              ''
          }
      '';
      buildPhase = ''
        export CARGO_BUILD_JOBS=2
        cmake --build build --parallel 2
      '';
      installPhase = ''
        cmake --install build
        mkdir -p "$out/share/korri-webos-viewer/licenses"
        cp ${ss4s}/LICENSE "$out/share/korri-webos-viewer/licenses/ss4s.txt"
        cp ${../android/app/src/main/jni/moonlight-core/moonlight-common-c}/LICENSE.txt "$out/share/korri-webos-viewer/licenses/common-c.txt"
        cp ${../android/app/src/main/jni/moonlight-core/moonlight-common-c}/enet/LICENSE "$out/share/korri-webos-viewer/licenses/enet.txt"
        # The Reed-Solomon notice is embedded at the top of this source file.
        cp ${../android/app/src/main/jni/moonlight-core/moonlight-common-c}/reedsolomon/rs.c "$out/share/korri-webos-viewer/licenses/reedsolomon.c"
        cp mbedtls-source/LICENSE "$out/share/korri-webos-viewer/licenses/mbedtls.txt"
        cp opus-source/COPYING "$out/share/korri-webos-viewer/licenses/opus.txt"
        cp expat-source/COPYING "$out/share/korri-webos-viewer/licenses/expat.txt"
        ${pkgs.lib.optionalString host ''
          export LD_LIBRARY_PATH="$out/lib"
          export VIEWER_BINARY="$out/bin/webos-viewer-test"
          python3 tests.py
          timeout 15 ./build/viewer-protocol-test
        ''}
      '';
    };
  native = build true;
  cross = build false;
  app =
    name: text:
    let
      script = pkgs.writeShellApplication {
        inherit name text;
        runtimeInputs = [
          pkgs.python3
          pkgs.binutils
        ];
      };
    in
    {
      type = "app";
      program = "${script}/bin/${name}";
    };
in
{
  inherit native cross;
  apps = {
    webos-viewer-check = app "webos-viewer-check" ''
      echo "Native CLI, loopback mutual TLS and permission checks: ${native}"
    '';
    webos-viewer-build = app "webos-viewer-build" ''
      echo "Prebuilt ARMv7 viewer: ${cross}"
      python3 ${./.}/abi_check.py ${cross}
      python3 ${./.}/symbol_audit.py ${cross} "''${KORRI_WEBOS_TV_LIBS:?set path to original tvlibs evidence}"
      export VIEWER_ARTIFACTS=${cross} TV_LIBRARY_EVIDENCE="$KORRI_WEBOS_TV_LIBS"
      python3 -m unittest discover -s ${./.} -p '*_test.py'
    '';
  };
  help = ''

    nix run .#webos-viewer-check
        Build/run workstation identity and local pinned-TLS tests, dummy media.
    KORRI_WEBOS_TV_LIBS=/path/to/tvlibs nix run .#webos-viewer-build
        Cross-build viewer library/test CLI; audit TV symbols. No deployment.'';
}

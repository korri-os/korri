# Workstation-only, partial M4 spike. Produces ARM artifacts; never executes them.
{ pkgs }:
let
  source =
    owner: repo: rev: hash:
    pkgs.fetchzip {
      url = "https://github.com/${owner}/${repo}/archive/${rev}.tar.gz";
      name = "${rev}.tar.gz";
      sha256 = hash;
    };
  common =
    source "moonlight-stream" "moonlight-common-c" "c86e0537d11566f9825d6323711ec61e3e702365"
      "1llr02lcbjp301yr7s36821cgrdq2d9n7cvh60q0l00cp2sq4hsx";
  enet =
    source "cgutman" "enet" "115a10baa1d7f291ff5b870765610fd3b4a6e43c"
      "0bv6lqgaxfv18f75f2a8kkqc1fg3h9ia77qf8i1rjfpr0q7m1j07";
  ss4s =
    source "mariotaku" "ss4s" "9cbad6a97b951aaf8d957b84f14b652ddb3dd5d2"
      "0d9hf0d7li713mnc27g1qrjzcmb5j9m8mxxy7y7r4an91mh24lxy";
  archive = pkgs.fetchurl {
    url = "https://github.com/openlgtv/buildroot-nc4/releases/download/2026.08-webos/arm-webos-linux-gnueabi_sdk-buildroot-x86_64.tar.gz";
    sha256 = "2230a6351e5cce6fcb9d4fdcb4697335f223995401ef969c81744aaeb14c5344";
  };
  # Same prebuilt SDK and host-loader preparation as slint-spike/webos-cross.sh.
  # This is a private immutable Nix extraction, not a shared SDK mutation. No
  # TV libc overlay is needed for this C/C++ link experiment (SDK glibc 2.12.2).
  sdk = pkgs.stdenvNoCC.mkDerivation {
    name = "korri-webos-viewer-spike-sdk";
    dontUnpack = true;
    dontFixup = true;
    nativeBuildInputs = [
      pkgs.patchelf
      pkgs.file
    ];
    buildCommand = ''
      mkdir -p "$out"
      tar xzf ${archive} --strip-components=1 -C "$out"
      (cd "$out" && bash ./relocate-sdk.sh)
      while IFS= read -r -d "" executable; do
        if file -b "$executable" | grep -q 'ELF 64-bit.*x86-64.*interpreter'; then
          patchelf --set-interpreter ${pkgs.stdenv.cc.libc}/lib/ld-linux-x86-64.so.2 "$executable"
          existing="$(patchelf --print-rpath "$executable")"
          patchelf --set-rpath "${
            pkgs.lib.makeLibraryPath [
              pkgs.stdenv.cc.cc.lib
              pkgs.stdenv.cc.libc
              pkgs.zlib
              pkgs.expat
            ]
          }''${existing:+:$existing}" "$executable"
        fi
      done < <(find "$out/bin" "$out/libexec" "$out/arm-webos-linux-gnueabi/bin" -type f -perm /111 -print0)
    '';
  };
  spike = pkgs.stdenvNoCC.mkDerivation {
    pname = "korri-webos-embedded-viewer-media-link-spike";
    version = "0";
    src = ./.;
    nativeBuildInputs = [
      pkgs.cmake
      pkgs.ninja
      pkgs.pkg-config
      pkgs.python3
      pkgs.binutils
    ];
    dontUseCmakeConfigure = true;
    dontFixup = true; # Native fixup must not rewrite any ARM ELF.
    configurePhase = ''
      runHook preConfigure
      cp -r ${common} common-source
      chmod -R u+w common-source
      cp -r ${enet}/. common-source/enet/
      export PKG_CONFIG_SYSROOT_DIR=${sdk}/arm-webos-linux-gnueabi/sysroot
      export PKG_CONFIG_LIBDIR="$PKG_CONFIG_SYSROOT_DIR/usr/lib/pkgconfig:$PKG_CONFIG_SYSROOT_DIR/usr/share/pkgconfig"
      export PKG_CONFIG_PATH=
      cmake -S . -B build -G Ninja \
        -DCMAKE_SYSTEM_NAME=Linux -DCMAKE_SYSTEM_PROCESSOR=armv7 \
        -DCMAKE_C_COMPILER=${sdk}/bin/arm-webos-linux-gnueabi-gcc \
        -DCMAKE_CXX_COMPILER=${sdk}/bin/arm-webos-linux-gnueabi-g++ \
        -DCMAKE_SYSROOT="$PKG_CONFIG_SYSROOT_DIR" \
        -DCMAKE_FIND_ROOT_PATH="$PKG_CONFIG_SYSROOT_DIR" \
        -DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER \
        -DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY \
        -DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY \
        -DCMAKE_C_FLAGS=-mfloat-abi=softfp -DCMAKE_CXX_FLAGS=-mfloat-abi=softfp \
        -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON \
        -DCMAKE_INSTALL_PREFIX="$out" \
        -DCOMMON_SOURCE="$PWD/common-source" -DSS4S_SOURCE=${ss4s}
      runHook postConfigure
    '';
    buildPhase = ''
      runHook preBuild
      export CARGO_BUILD_JOBS=2
      cmake --build build --parallel 2
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      cmake --install build
      mkdir -p "$out/share/korri-webos-viewer-spike/licenses"
      cp ${common}/LICENSE.txt "$out/share/korri-webos-viewer-spike/licenses/moonlight-common-c.txt"
      cp ${ss4s}/LICENSE "$out/share/korri-webos-viewer-spike/licenses/ss4s.txt"
      cp ${enet}/LICENSE "$out/share/korri-webos-viewer-spike/licenses/enet.txt"
      cp README.md "$out/share/korri-webos-viewer-spike/"
      python3 check_elf.py "$out" > "$out/share/korri-webos-viewer-spike/elf-report.txt"
      SPIKE_ARTIFACTS="$out" python3 -m unittest check_elf_test.py
      runHook postInstall
    '';
  };
  app = pkgs.writeShellApplication {
    name = "webos-embedded-viewer-spike-build";
    text = ''
      printf 'Partial media link spike (not a stream viewer): %s\n' ${spike}
      cat ${spike}/share/korri-webos-viewer-spike/elf-report.txt
    '';
  };
in
{
  apps.webos-embedded-viewer-spike-build = {
    type = "app";
    program = "${app}/bin/webos-embedded-viewer-spike-build";
  };
  help = ''

    nix run .#webos-embedded-viewer-spike-build
        Cross-link pinned common-c and SMP webOS 3 in Nix; inspect ARM ELFs.
        Partial research spike only: no playback, target execution or IPK.'';
}

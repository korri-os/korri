# Experiment 2: workstation-built local video test. Never executes ARM or contacts a TV.
{ pkgs }:
let
  # Read-only original TV copies are a content-addressed build input, never
  # bundled. Set this operator path explicitly and evaluate with --impure.
  evidencePath = builtins.getEnv "KORRI_WEBOS_TV_LIBS";
  tvEvidence =
    if evidencePath == "" then
      throw "Set KORRI_WEBOS_TV_LIBS to the TV library evidence directory and use nix run --impure"
    else
      builtins.path {
        path = builtins.toPath evidencePath;
        name = "korri-webos-tv-library-evidence";
      };
  source =
    owner: repo: rev: hash:
    pkgs.fetchzip {
      url = "https://github.com/${owner}/${repo}/archive/${rev}.tar.gz";
      name = "${rev}.tar.gz";
      sha256 = hash;
    };
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
  stream =
    pkgs.runCommand "korri-webos-test-1080p30"
      {
        nativeBuildInputs = [ pkgs.ffmpeg ];
      }
      ''
        mkdir -p "$out"
        ffmpeg -hide_banner -loglevel warning -nostdin \
          -filter_threads 1 -f lavfi -i 'testsrc2=size=1920x1080:rate=30' \
          -vf "drawtext=fontfile=${pkgs.dejavu_fonts}/share/fonts/truetype/DejaVuSans.ttf:text='frame %{n}':x=80:y=80:fontsize=96:fontcolor=white:box=1:boxcolor=black" \
          -frames:v 300 -r 30 -fps_mode cfr -an -c:v libx264 -profile:v baseline -level:v 4.0 \
          -preset veryfast -crf 28 -maxrate 5M -bufsize 5M -pix_fmt yuv420p \
          -threads 2 -x264-params 'force-cfr=1:threads=2:lookahead-threads=1:sliced-threads=0:slices=1:aud=1:repeat-headers=1:keyint=30:min-keyint=30:scenecut=0:bframes=0' \
          -fflags +bitexact -flags:v +bitexact -map_metadata -1 \
          -f h264 "$out/test-1080p30.h264"
        test "$(stat -c %s "$out/test-1080p30.h264")" -le 10485760
        # Raw H.264's r_frame_rate reports the 60 Hz field clock, not picture
        # cadence. Validate the decoder's actual per-picture durations instead.
        ffprobe -v error -threads 2 -select_streams v:0 -count_frames -show_frames \
          -show_entries stream=codec_name,profile,width,height,r_frame_rate,nb_read_frames:frame=duration_time \
          -of default=noprint_wrappers=1 "$out/test-1080p30.h264" > decoded.txt
        test "$(grep -c '^duration_time=0.033333$' decoded.txt)" -eq 300
        grep -v '^duration_time=' decoded.txt > "$out/stream-report.txt"
        echo 'decoded_picture_rate=30/1' >> "$out/stream-report.txt"
        grep -qx 'codec_name=h264' "$out/stream-report.txt"
        grep -qx 'profile=Constrained Baseline' "$out/stream-report.txt"
        grep -qx 'width=1920' "$out/stream-report.txt"
        grep -qx 'height=1080' "$out/stream-report.txt"
        grep -qx 'nb_read_frames=300' "$out/stream-report.txt"
      '';
  nativeTests = pkgs.stdenv.mkDerivation {
    name = "korri-webos-media-test-native-checks";
    src = ./.;
    nativeBuildInputs = [
      pkgs.cmake
      pkgs.ninja
      pkgs.python3
    ];
    dontUseCmakeConfigure = true;
    configurePhase = ''
      cp -r ${ss4s} ss4s-native-source
      chmod -R u+w ss4s-native-source
      patch -d ss4s-native-source -p1 < ss4s-reference-close-test.patch
      patch -d ss4s-native-source -p1 < ss4s-media-test.patch
      cmake -S . -B build -G Ninja -DSS4S_SOURCE="$PWD/ss4s-native-source" -DMEDIA_HOST_TEST=ON
    '';
    buildPhase = ''
      cmake --build build --parallel 2
    '';
    installPhase = ''
      export APPID=org.korri.shell
      export LD_LIBRARY_PATH="$PWD/build"
      export MEDIA_TEST_BINARY="$PWD/build/media-test"
      export MEDIA_TEST_STREAM=${stream}/test-1080p30.h264
      python3 -m unittest media_test_test.py
      mkdir "$out"
      touch "$out/passed"
    '';
  };
  spike = pkgs.stdenvNoCC.mkDerivation {
    pname = "korri-webos-embedded-viewer-media-test";
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
      cp -r ${ss4s} ss4s-source
      chmod -R u+w ss4s-source
      patch -d ss4s-source -p1 < ss4s-media-test.patch
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
        -DSS4S_SOURCE="$PWD/ss4s-source"
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
      cp ${ss4s}/LICENSE "$out/share/korri-webos-viewer-spike/licenses/ss4s.txt"
      cp README.md ss4s-media-test.patch "$out/share/korri-webos-viewer-spike/"
      cp ${stream}/* "$out/share/korri-webos-viewer-spike/"
      cp run-media-test "$out/bin/"
      test -e ${nativeTests}/passed
      python3 check_elf.py "$out" > "$out/share/korri-webos-viewer-spike/elf-report.txt"
      export TV_LIBRARY_EVIDENCE=${tvEvidence}
      python3 symbol_audit.py "$out" "$TV_LIBRARY_EVIDENCE" > "$out/share/korri-webos-viewer-spike/symbol-report.txt"
      SPIKE_ARTIFACTS="$out" python3 -m unittest check_elf_test.py symbol_audit_test.py
      (cd "$out" && find bin lib share -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum) > "$out/SHA256SUMS"
      runHook postInstall
    '';
  };
  app = pkgs.writeShellApplication {
    name = "webos-embedded-viewer-spike-build";
    text = ''
      printf 'Local video experiment (not M4 playback): %s\n' ${spike}
      cat ${spike}/share/korri-webos-viewer-spike/elf-report.txt
      cat ${spike}/share/korri-webos-viewer-spike/symbol-report.txt
    '';
  };
in
{
  inherit
    sdk
    ss4s
    nativeTests
    stream
    ;
  apps.webos-embedded-viewer-spike-build = {
    type = "app";
    program = "${app}/bin/webos-embedded-viewer-spike-build";
  };
  help = ''

    KORRI_WEBOS_TV_LIBS=/path/to/tvlibs nix run --impure .#webos-embedded-viewer-spike-build
        Build video-only NDL webOS 4 test and 1080p30 fixture; inspect ARM ELFs.
        No network/identity, target execution or IPK.'';
}

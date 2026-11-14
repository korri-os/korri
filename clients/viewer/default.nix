{ pkgs }:
let
  # Exactly the network core used by the plugin's pinned Moonlight. Its gitlink
  # is b126e481a195fdc7152d211def17190e3434bcce, including that pin's ENet.
  upstream = pkgs.fetchFromGitHub {
    owner = "moonlight-stream";
    repo = "moonlight-embedded";
    rev = "775444287305849ebdf4736c75298ad0713e2d5d";
    hash = "sha256-h+hI9TQUGMI8VDzvnuRGDjxm8D7GuC/L4/0rwTFocgA=";
    fetchSubmodules = true;
  };
  network = pkgs.stdenv.mkDerivation {
    pname = "korri-viewer-moonlight-common-c";
    version = "b126e481";
    src = "${upstream}/third_party/moonlight-common-c";
    nativeBuildInputs = [
      pkgs.cmake
      pkgs.pkg-config
    ];
    buildInputs = [ pkgs.openssl ];
    cmakeFlags = [ "-DCMAKE_BUILD_TYPE=Release" ];
    installPhase = ''
      mkdir -p $out/lib $out/include
      cp libmoonlight-common-c.so $out/lib/
      cp ../src/Limelight.h $out/include/
    '';
  };
  opusStandardModes = pkgs.libopus.override { withCustomModes = false; };
  # The handheld path is aarch64 only. Its decoder needs FFmpeg's V4L2
  # Request hwaccel, which upstream FFmpeg does not carry. This is the
  # `@korri:moonlight` package's ffmpegV4l2Request, copied from
  # korri-plugins services/moonlight/package.nix at 2094c1e77471 with its
  # patch byte for byte, on this repository's nixpkgs FFmpeg. The PC paths
  # keep the unpatched FFmpeg.
  request = pkgs.stdenv.hostPlatform.isAarch64;
  ffmpegV4l2Request = (pkgs.ffmpeg.override { libopus = opusStandardModes; }).overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ./ffmpeg/0001-v4l2-request.patch ];
    configureFlags = (old.configureFlags or [ ]) ++ [
      "--enable-libudev"
      "--enable-v4l2-request"
    ];
    buildInputs = (old.buildInputs or [ ]) ++ [ pkgs.systemdMinimal ];
  });
  ffmpeg = if request then ffmpegV4l2Request else pkgs.ffmpeg;
  libraries =
    with pkgs;
    [
      openssl
      curl
      ffmpeg
      libva
      libX11
      libglvnd
      SDL2
      alsa-lib
      libpulseaudio
      libevdev
      systemdMinimal
      opusStandardModes
    ]
    # drm_fourcc.h for the v4l2request presenter.
    ++ pkgs.lib.optional request pkgs.libdrm;
  native = with pkgs; [
    pkg-config
    clang
    rustPlatform.bindgenHook
  ];
  environment = {
    MOONLIGHT_COMMON_INCLUDE = "${network}/include";
    MOONLIGHT_COMMON_LIB = "${network}/lib";
  };
  repositorySource = pkgs.lib.fileset.toSource {
    root = ../..;
    fileset = pkgs.lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./build.rs
      ./LICENSE
      ./src
      ./picture
      ./io
      ./tests
      ../../contracts/viewer/Cargo.toml
      ../../contracts/viewer/src
      ../../contracts/surface/Cargo.toml
      ../../contracts/surface/src
      # korrid refuses @korri:viewer unless this file equals its own copy.
      ../../contracts/generated/viewer.ts
    ];
  };
  source = "${repositorySource}/clients/viewer";
  package = pkgs.rustPlatform.buildRustPackage (
    environment
    // {
      pname = "korri-viewer";
      version = "0.1.0";
      src = repositorySource;
      sourceRoot = "source/clients/viewer";
      cargoLock.lockFile = ./Cargo.lock;
      nativeBuildInputs = native;
      buildInputs = libraries ++ [ network ];
      preCheck = ''
        export XDG_RUNTIME_DIR="$TMPDIR/viewer-runtime"
        mkdir -m 700 "$XDG_RUNTIME_DIR"
        export CARGO_TARGET_${pkgs.stdenv.hostPlatform.rust.cargoEnvVarTarget}_RUNNER="${pkgs.xvfb-run}/bin/xvfb-run -a"
        export SDL_VIDEODRIVER=x11 SDL_RENDER_DRIVER=software
        export LIBGL_ALWAYS_SOFTWARE=1 __GLX_VENDOR_LIBRARY_NAME=mesa LIBGL_DRIVERS_PATH=${pkgs.mesa}/lib/dri
        export LD_LIBRARY_PATH=${
          pkgs.lib.makeLibraryPath [
            network
            pkgs.libglvnd
            pkgs.mesa
          ]
        }
      '';
      postInstall = ''
        # The unchanged stage-0 tool asks every candidate package for this name.
        ln -s korri-viewer $out/bin/moonlight
        mkdir -p $out/share/moonlight
        cp ${upstream}/third_party/SDL_GameControllerDB/gamecontrollerdb.txt \
          $out/share/moonlight/korri-inputplumber-gamecontrollerdb.txt
        korriInputplumberMapping="$out/share/moonlight/korri-inputplumber-gamecontrollerdb.txt"
        substituteInPlace "$korriInputplumberMapping" \
          --replace-fail '030000005e0400008e02000001000000,Microsoft Xbox 360,a:b0,b:b1,back:b6,dpdown:h0.1,dpleft:h0.2,dpright:h0.8,dpup:h0.4,leftshoulder:b4,leftstick:b9,lefttrigger:a2,leftx:a0,lefty:a1,rightshoulder:b5,rightstick:b10,righttrigger:a5,rightx:a3,righty:a4,start:b7,x:b2,y:b3,platform:Linux,' \
          '030000005e0400008e02000001000000,Microsoft Xbox 360,a:b0,b:b1,back:b6,dpdown:h0.4,dpleft:h0.8,dpright:h0.2,dpup:h0.1,leftshoulder:b4,leftstick:b9,lefttrigger:a2,leftx:a0,lefty:a1,rightshoulder:b5,rightstick:b10,righttrigger:a5,rightx:a3,righty:a4,start:b7,x:b2,y:b3,platform:Linux,'
        printf '%s\n' \
          '030000005e040000120b000001000000,Microsoft Xbox Series S|X Controller,a:b0,b:b1,x:b2,y:b3,back:b6,start:b7,guide:b8,leftstick:b9,rightstick:b10,leftshoulder:b4,rightshoulder:b5,dpup:h0.1,dpright:h0.2,dpdown:h0.4,dpleft:h0.8,leftx:a0,lefty:a1,rightx:a3,righty:a4,lefttrigger:a2,righttrigger:a5,platform:Linux,' \
          >> "$korriInputplumberMapping"
        mkdir -p $out/share/licenses/korri-viewer
        cp LICENSE $out/share/licenses/korri-viewer/
        # The wire contract this binary speaks, as Typeshare generated it. The
        # plugin declares it as files.treaty; korrid compares the bytes.
        install -Dm0444 ../../contracts/generated/viewer.ts $out/share/korri-viewer/viewer.ts
      '';
      # The FFmpeg the viewer links, for the aarch64 check.
      passthru = { inherit ffmpeg; };
      meta = {
        license = pkgs.lib.licenses.gpl3Plus;
        mainProgram = "korri-viewer";
        platforms = [
          "x86_64-linux"
          "aarch64-linux"
        ];
      };
    }
  );
  ioSources = pkgs.lib.concatMapStringsSep " " (file: "${source}/io/${file}") [
    "bridge.c"
    "audio/alsa.c"
    "audio/pulse.c"
    "audio/sdl.c"
    "input/evdev.c"
    "input/udev.c"
    "input/mapping.c"
    "input/sdl.c"
    "input/x11.c"
  ];
  ioFlags = "-DHAVE_X11 -DHAVE_VAAPI -DHAVE_SDL -DHAVE_ALSA -DHAVE_PULSE -I${source} -I${network}/include -ffunction-sections -fdata-sections -Wl,--gc-sections";
  ioLibraries = "opus alsa libpulse-simple libevdev libudev sdl2 x11";
  ioCheck =
    pkgs.runCommand "korri-viewer-io-check"
      {
        nativeBuildInputs = [
          pkgs.stdenv.cc
          pkgs.pkg-config
          pkgs.xvfb-run
          pkgs.strace
        ];
        buildInputs = libraries ++ [ network ];
      }
      ''
        cc -std=gnu11 ${ioFlags} ${source}/tests/test-audio.c ${ioSources} \
          $(pkg-config --cflags --libs ${ioLibraries}) -lm -pthread -o check-audio
        export HOME="$TMPDIR" PULSE_SERVER=unix:/no-pulse-server
        cat > alsa.conf <<EOF
        pcm.null { type null }
        pcm.korri_test { type file slave.pcm null file "$TMPDIR/decoded.pcm" format raw }
        EOF
        export ALSA_CONFIG_PATH="$PWD/alsa.conf"
        ./check-audio "$TMPDIR/decoded.pcm"
        cc -std=gnu11 ${ioFlags} -I${source}/picture \
          ${source}/tests/test-input.c ${ioSources} \
          $(pkg-config --cflags --libs ${ioLibraries}) \
          -L${network}/lib -lmoonlight-common-c -pthread -lm \
          -Wl,--wrap=LiSendKeyboardEvent,--wrap=LiSendMouseButtonEvent,--wrap=LiSendMousePositionEvent,--wrap=LiSendMouseMoveEvent,--wrap=LiSendMultiControllerEvent,--wrap=udev_new \
          -o check-input
        cc -std=gnu11 ${ioFlags} -I${source}/picture \
          ${source}/tests/test-x11-input.c ${source}/picture/bridge.c \
          $(pkg-config --cflags --libs ${ioLibraries}) \
          -L${network}/lib -lmoonlight-common-c -pthread -lm \
          -Wl,--wrap=LiSendKeyboardEvent,--wrap=LiSendMouseButtonEvent,--wrap=LiSendMouseMoveEvent,--wrap=LiSendScrollEvent \
          -o check-x11-input
        cc -std=gnu11 ${ioFlags} -I${source}/picture \
          ${source}/tests/test-evdev-input.c ${source}/io/input/mapping.c ${source}/picture/bridge.c \
          $(pkg-config --cflags --libs ${ioLibraries}) \
          -L${network}/lib -lmoonlight-common-c -pthread -lm \
          -Wl,--wrap=LiSendKeyboardEvent,--wrap=LiSendMouseButtonEvent,--wrap=LiSendMouseMoveEvent,--wrap=LiSendMultiControllerEvent,--wrap=calloc,--wrap=realloc \
          -o check-evdev-input
        ./check-evdev-input ${upstream}/third_party/SDL_GameControllerDB/gamecontrollerdb.txt
        xvfb-run -a ./check-x11-input
        export LD_LIBRARY_PATH=${
          pkgs.lib.makeLibraryPath [
            network
            pkgs.libglvnd
            pkgs.mesa
          ]
        }
        export LIBGL_ALWAYS_SOFTWARE=1 __GLX_VENDOR_LIBRARY_NAME=mesa LIBGL_DRIVERS_PATH=${pkgs.mesa}/lib/dri
        export SDL_VIDEODRIVER=x11 SDL_RENDER_DRIVER=software
        xvfb-run -a ./check-input
        xvfb-run -a strace -f -e trace=open,openat,openat2 -o viewonly.trace ./check-input viewonly
        if grep '/dev/input' viewonly.trace; then
          echo 'viewonly attempted input discovery' >&2
          exit 1
        fi
        mkdir -p "$out"
        cp viewonly.trace "$out/"
      '';
  pictureCheck =
    pkgs.runCommand "korri-viewer-picture-check"
      {
        nativeBuildInputs = [
          pkgs.stdenv.cc
          pkgs.pkg-config
          pkgs.xvfb-run
          pkgs.ffmpeg
        ];
        buildInputs = libraries;
      }
      ''
          cc -Wall -Wextra -Werror -Wno-unused-parameter -I${source}/picture \
            ${source}/picture/video/ffmpeg_vaapi.c ${source}/tests/test-vaapi.c \
            $(pkg-config --cflags --libs libavcodec libavutil libva-x11 x11) -o check-vaapi
          export HOME="$TMPDIR" LIBVA_DRIVER_NAME=korri_missing_driver
          xvfb-run -a -s '-screen 0 640x480x24 -nolisten tcp' ./check-vaapi
          cc -std=gnu11 -DHAVE_X11 -DHAVE_VAAPI \
            -ffunction-sections -fdata-sections -Wl,--gc-sections \
            -I${source}/picture -I${network}/include \
            ${source}/tests/test-x11-vaapi-size.c ${source}/picture/video/egl.c \
            $(pkg-config --cflags --libs libavcodec libavutil libva-x11 x11 egl glesv2) -o check-size
          xvfb-run -a -s '-screen 0 4096x2304x24 -nolisten tcp' ./check-size
          cc -std=gnu11 -DHAVE_SDL -I${source}/picture -I${network}/include \
          ${ioFlags} ${source}/tests/test-sdl.c ${ioSources} ${source}/picture/bridge.c ${source}/picture/video/sdl.c \
          ${source}/picture/video/ffmpeg.c ${source}/picture/video/ffmpeg_vaapi.c ${source}/picture/util.c \
          $(pkg-config --cflags --libs libavcodec libavutil libva-x11 ${ioLibraries}) -L${network}/lib -lmoonlight-common-c -pthread -lm -o check-sdl
        ffmpeg -v error -f lavfi -i testsrc=size=128x72:rate=30 -frames:v 4 \
          -c:v libx264 -pix_fmt yuv420p -tune zerolatency -f h264 frames.h264
        LD_LIBRARY_PATH=${
          pkgs.lib.makeLibraryPath [
            network
            pkgs.libglvnd
            pkgs.mesa
          ]
        } \
          LIBGL_ALWAYS_SOFTWARE=1 __GLX_VENDOR_LIBRARY_NAME=mesa LIBGL_DRIVERS_PATH=${pkgs.mesa}/lib/dri \
          SDL_VIDEODRIVER=x11 SDL_RENDER_DRIVER=software \
          xvfb-run -a -s '-screen 0 640x480x24 -nolisten tcp' ./check-sdl frames.h264
        touch "$out"
      '';
  # GPL-3 corresponding source of `package` (ADR 0059): this package's Rust,
  # C and recipe, the network core with its ENet, and the vendored crates.
  # The plugin carries it beside the binary. nixpkgs' shared libraries are
  # system libraries here and come with their own sources from nixpkgs.
  correspondingSource =
    pkgs.runCommand "korri-viewer-corresponding-source-${package.version}"
      {
        nativeBuildInputs = [
          pkgs.gnutar
          pkgs.xz
        ];
      }
      ''
        root=korri-viewer-source
        mkdir -p $root/korri $root/moonlight-common-c $root/vendor
        cp -r ${repositorySource}/. $root/korri/
        chmod -R u+w $root/korri
        cp ${./default.nix} $root/korri/clients/viewer/default.nix
        cp ${./README.md} $root/korri/clients/viewer/README.md
        # The FFmpeg patch of the aarch64 build; nixpkgs FFmpeg is its base.
        cp -r ${./ffmpeg} $root/korri/clients/viewer/ffmpeg
        cp -r ${upstream}/third_party/moonlight-common-c/. $root/moonlight-common-c/
        # The vendor directory links each crate; the archive holds their files.
        cp -rL ${package.cargoDeps}/. $root/vendor/
        cat > $root/BUILDING.txt <<'EOF'
        korri-viewer ${package.version}, GPL-3.0-or-later.

        korri/                The Korri repository files that build the viewer.
                              clients/viewer/default.nix is the Nix recipe.
        moonlight-common-c/   The network core at b126e481a195fdc7152d211def17190e3434bcce,
                              with its ENet, from Moonlight Embedded
                              775444287305849ebdf4736c75298ad0713e2d5d.
        vendor/               The Rust crates of korri/clients/viewer/Cargo.lock.

        On aarch64 the viewer links nixpkgs FFmpeg with
        korri/clients/viewer/ffmpeg/0001-v4l2-request.patch, as default.nix says.

        Build the network core with CMake, then run cargo build --release
        --offline in korri/clients/viewer with MOONLIGHT_COMMON_INCLUDE and
        MOONLIGHT_COMMON_LIB set to its headers and shared library, and
        vendor/ configured as the crates.io source.
        EOF
        mkdir -p $out/share/korri-viewer
        tar --sort=name --mtime=@1 --owner=0 --group=0 --numeric-owner \
          -cJf $out/share/korri-viewer/korri-viewer-source.tar.xz $root
      '';
in
{
  inherit
    package
    network
    pictureCheck
    ioCheck
    correspondingSource
    ;
  devShell = pkgs.mkShell (
    environment
    // {
      nativeBuildInputs =
        native
        ++ (with pkgs; [
          cargo
          rustc
          rustfmt
          clippy
          xvfb-run
        ]);
      buildInputs = libraries;
      LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
        network
        pkgs.libglvnd
        pkgs.mesa
      ];
      LIBGL_DRIVERS_PATH = "${pkgs.mesa}/lib/dri";
    }
  );
}

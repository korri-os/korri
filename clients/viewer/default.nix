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
  libraries = with pkgs; [
    openssl
    curl
    ffmpeg
    libva
    libX11
    libglvnd
    SDL2
  ];
  native = with pkgs; [
    pkg-config
    clang
    rustPlatform.bindgenHook
  ];
  environment = {
    MOONLIGHT_COMMON_INCLUDE = "${network}/include";
    MOONLIGHT_COMMON_LIB = "${network}/lib";
  };
  package = pkgs.rustPlatform.buildRustPackage (
    environment
    // {
      pname = "korri-viewer";
      version = "0.1.0";
      src = pkgs.lib.fileset.toSource {
        root = ../..;
        fileset = pkgs.lib.fileset.unions [
          ./Cargo.toml
          ./Cargo.lock
          ./build.rs
          ./LICENSE
          ./src
          ./picture
          ./tests
          ../../contracts/viewer/Cargo.toml
          ../../contracts/viewer/src
          ../../contracts/surface/Cargo.toml
          ../../contracts/surface/src
        ];
      };
      sourceRoot = "source/clients/viewer";
      cargoLock.lockFile = ./Cargo.lock;
      nativeBuildInputs = native;
      buildInputs = libraries ++ [ network ];
      postInstall = ''
        # The unchanged stage-0 tool asks every candidate package for this name.
        ln -s korri-viewer $out/bin/moonlight
        mkdir -p $out/share/moonlight
        cp ${upstream}/third_party/SDL_GameControllerDB/gamecontrollerdb.txt \
          $out/share/moonlight/korri-inputplumber-gamecontrollerdb.txt
        mkdir -p $out/share/licenses/korri-viewer
        cp LICENSE $out/share/licenses/korri-viewer/
      '';
      meta = {
        license = pkgs.lib.licenses.gpl3Plus;
        mainProgram = "korri-viewer";
        platforms = [ "x86_64-linux" ];
      };
    }
  );
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
          cc -Wall -Wextra -Werror -Wno-unused-parameter -I${./picture} \
            ${./picture}/video/ffmpeg_vaapi.c ${./tests/test-vaapi.c} \
            $(pkg-config --cflags --libs libavcodec libavutil libva-x11 x11) -o check-vaapi
          export HOME="$TMPDIR" LIBVA_DRIVER_NAME=korri_missing_driver
          xvfb-run -a -s '-screen 0 640x480x24 -nolisten tcp' ./check-vaapi
          cc -std=gnu11 -DHAVE_X11 -DHAVE_VAAPI \
            -ffunction-sections -fdata-sections -Wl,--gc-sections \
            -I${./picture} -I${network}/include \
            ${./tests/test-x11-vaapi-size.c} ${./picture}/video/egl.c \
            $(pkg-config --cflags --libs libavcodec libavutil libva-x11 x11 egl glesv2) -o check-size
          xvfb-run -a -s '-screen 0 4096x2304x24 -nolisten tcp' ./check-size
          cc -std=gnu11 -DHAVE_SDL -I${./picture} -I${network}/include \
          ${./tests/test-sdl.c} ${./picture}/video/sdl.c \
          ${./picture}/video/ffmpeg.c ${./picture}/util.c \
          $(pkg-config --cflags --libs libavcodec libavutil sdl2) -o check-sdl
        ffmpeg -v error -f lavfi -i testsrc=size=128x72:rate=30 -frames:v 4 \
          -c:v libx264 -pix_fmt yuv420p -tune zerolatency -f h264 frames.h264
        LD_LIBRARY_PATH=${
          pkgs.lib.makeLibraryPath [
            pkgs.libglvnd
            pkgs.mesa
          ]
        } \
          LIBGL_ALWAYS_SOFTWARE=1 __GLX_VENDOR_LIBRARY_NAME=mesa LIBGL_DRIVERS_PATH=${pkgs.mesa}/lib/dri \
          SDL_VIDEODRIVER=x11 SDL_RENDER_DRIVER=software \
          xvfb-run -a -s '-screen 0 640x480x24 -nolisten tcp' ./check-sdl frames.h264
        touch "$out"
      '';
in
{
  inherit package network pictureCheck;
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
        ]);
      buildInputs = libraries;
      LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ network ];
    }
  );
}

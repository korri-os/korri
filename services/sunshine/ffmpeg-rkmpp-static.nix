{
  stdenv,
  lib,
  fetchFromGitHub,
  cmake,
  pkg-config,
  nasm,
  perl,
  libdrm,
  rockchipMpp,
}:

let
  approved = import ./approved-patches.nix;
  # Sunshine links against a static FFmpeg whose ABI is reviewed in
  # approved-patches.nix. Build the RKMPP encoder against that exact commit
  # so the nvenc/vaapi/x86 paths keep the same libavcodec contract.
  ffmpegSrc = fetchFromGitHub {
    owner = "FFmpeg";
    repo = "FFmpeg";
    rev = approved.reviewedFfmpegCommit;
    hash = approved.reviewedFfmpegSourceHash;
  };
in
stdenv.mkDerivation {
  pname = "sunshine-ffmpeg-rkmpp";
  version = "8.0-${builtins.substring 0 7 approved.reviewedFfmpegCommit}";

  src = fetchFromGitHub {
    owner = "LizardByte";
    repo = "build-deps";
    rev = "2851db1";
    hash = "sha256-ojpcgvn2DItXQp1lqrL4eVdv0MXwcAo0eGfcqzZQvz4=";
  };

  nativeBuildInputs = [
    cmake
    pkg-config
    nasm
    perl
  ];

  buildInputs = [
    libdrm
    rockchipMpp
  ];

  postPatch = ''
    # build-deps expects FFmpeg as a git submodule. Replace the empty submodule
    # directory with the reviewed source tree.
    mkdir -p third-party/FFmpeg
    rm -rf third-party/FFmpeg/FFmpeg
    cp -R ${ffmpegSrc} third-party/FFmpeg/FFmpeg
    chmod -R u+w third-party/FFmpeg/FFmpeg
    patch -d third-party/FFmpeg/FFmpeg -p1 \
      < ${./patches/ffmpeg/0001-add-rkmpp-h264-encoder.patch}
    patch -d third-party/FFmpeg/FFmpeg -p1 \
      < ${./patches/ffmpeg/0002-adapt-rkmpp-to-reviewed-ffmpeg.patch}
    # APPLY_GIT_PATCH uses `git apply`, which silently no-ops outside a git
    # checkout. Apply Sunshine's CBS patches with plain patch instead.
    for cbsPatch in patches/FFmpeg/FFmpeg/cbs/*.patch; do
      patch -d third-party/FFmpeg/FFmpeg -p1 < "$cbsPatch"
    done

    substituteInPlace cmake/ffmpeg/ffmpeg.cmake \
      --replace-fail '        --enable-gpl' \
                     '        --enable-gpl;--enable-version3;--enable-libdrm;--enable-rkmpp;--enable-encoder=h264_rkmpp'
  '';

  cmakeFlags = [
    (lib.cmakeBool "BUILD_ALL" false)
    (lib.cmakeBool "BUILD_FFMPEG" true)
    (lib.cmakeBool "BUILD_FFMPEG_ALL_PATCHES" false)
    (lib.cmakeBool "BUILD_FFMPEG_CBS_PATCHES" true)
    (lib.cmakeBool "BUILD_FFMPEG_AMF" false)
    (lib.cmakeBool "BUILD_FFMPEG_MF" false)
    (lib.cmakeBool "BUILD_FFMPEG_NV_CODEC_HEADERS" false)
    (lib.cmakeBool "BUILD_FFMPEG_SVT_AV1" false)
    (lib.cmakeBool "BUILD_FFMPEG_VAAPI" false)
    (lib.cmakeBool "BUILD_FFMPEG_X264" false)
    (lib.cmakeBool "BUILD_FFMPEG_X265" false)
  ];

  enableParallelBuilding = true;

  postInstall = ''
    test -f "$out/lib/libavcodec.a"
    test -f "$out/lib/libavutil.a"
    test -f "$out/lib/libswscale.a"
    test -f "$out/lib/libcbs.a"
    grep -F AV_HWDEVICE_TYPE_RKMPP "$out/include/libavutil/hwcontext.h"
  '';

  meta = {
    description = "Sunshine-compatible static FFmpeg with RKMPP H.264 encoding";
    homepage = "https://github.com/LizardByte/build-deps";
    license = lib.licenses.gpl3Plus;
    platforms = [ "aarch64-linux" ];
  };
}

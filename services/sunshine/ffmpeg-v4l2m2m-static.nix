{
  stdenv,
  lib,
  fetchFromGitHub,
  cmake,
  ninja,
  pkg-config,
  nasm,
  perl,
  gitMinimal,
}:

let
  approved = import ./approved-patches.nix;
  buildDepsRev = approved.reviewedBuildDepsCommit;
  patchesApproved = builtins.all (
    record: builtins.hashFile "sha256" record.path == record.sha256
  ) approved.v4l2m2mPatches;
  ffmpegSrc = fetchFromGitHub {
    owner = "FFmpeg";
    repo = "FFmpeg";
    rev = approved.reviewedFfmpegCommit;
    hash = approved.reviewedFfmpegSourceHash;
  };
in
assert patchesApproved;
stdenv.mkDerivation {
  pname = "sunshine-ffmpeg-v4l2m2m-static";
  version = "${approved.reviewedLibavcodecVersion}-${
    builtins.substring 0 12 approved.reviewedFfmpegCommit
  }";

  src = fetchFromGitHub {
    owner = "LizardByte";
    repo = "build-deps";
    rev = buildDepsRev;
    hash = approved.reviewedBuildDepsSourceHash;
  };

  nativeBuildInputs = [
    cmake
    ninja
    pkg-config
    nasm
    perl
    gitMinimal
  ];

  postPatch = ''
    rm -rf third-party/FFmpeg/FFmpeg
    mkdir -p third-party/FFmpeg/FFmpeg
    cp -r ${ffmpegSrc}/. third-party/FFmpeg/FFmpeg/
    chmod -R u+w third-party/FFmpeg/FFmpeg

    ${lib.concatMapStringsSep "\n" (record: ''
      patch -d third-party/FFmpeg/FFmpeg -p1 < ${record.path}
    '') approved.v4l2m2mPatches}

    substituteInPlace cmake/ffmpeg/ffmpeg.cmake \
      --replace-fail \
        '--enable-encoder=h264_v4l2m2m' \
        '--enable-encoder=h264_v4l2m2m,hevc_v4l2m2m'
  '';

  cmakeFlags = [
    "-DCMAKE_BUILD_TYPE=Release"
    "-DBUILD_ALL=OFF"
    "-DBUILD_FFMPEG=ON"
    "-DBUILD_FFMPEG_ALL_PATCHES=OFF"
    "-DBUILD_FFMPEG_AMF=OFF"
    "-DBUILD_FFMPEG_CBS=ON"
    "-DBUILD_FFMPEG_CBS_PATCHES=ON"
    "-DBUILD_FFMPEG_MF=OFF"
    "-DBUILD_FFMPEG_NV_CODEC_HEADERS=OFF"
    "-DBUILD_FFMPEG_SVT_AV1=OFF"
    "-DBUILD_FFMPEG_VAAPI=OFF"
    "-DBUILD_FFMPEG_X264=OFF"
    "-DBUILD_FFMPEG_X265=OFF"
  ];

  passthru = {
    inherit buildDepsRev;
    ffmpegCommit = approved.reviewedFfmpegCommit;
    libavcodecVersion = approved.reviewedLibavcodecVersion;
    patchSetSha256 = approved.v4l2m2mPatchSetSha256;
  };

  meta = {
    description = "Static FFmpeg and CBS libraries for Sunshine V4L2 M2M encoding";
    homepage = "https://github.com/LizardByte/build-deps";
    license = lib.licenses.gpl3Plus;
    platforms = [ "aarch64-linux" ];
  };
}

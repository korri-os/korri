{
  pkgs,
  sunshinePackage,
  sunshineV4l2m2mPackage ? null,
  approvedPatchesPath,
  patchPath,
  ffmpegPatchPath,
  ffmpegPackagePath,
  packagePath,
  readmePath,
}:

let
  expectV4l2m2mPackage = pkgs.stdenv.hostPlatform.isAarch64;
  approved = import approvedPatchesPath;
  ffmpegSource = pkgs.applyPatches {
    name = "sunshine-v4l2m2m-ffmpeg-source";
    src = pkgs.fetchFromGitHub {
      owner = "FFmpeg";
      repo = "FFmpeg";
      rev = approved.reviewedFfmpegCommit;
      hash = approved.reviewedFfmpegSourceHash;
    };
    patches = map (record: record.path) approved.v4l2m2mPatches;
  };
  patchedSource = pkgs.applyPatches {
    name = "sunshine-v4l2m2m-source";
    src = pkgs.sunshine.src;
    patches = map (record: record.path) approved.patches;
  };
in
assert !(sunshinePackage.korriV4l2m2mEnabled or false);
assert builtins.elem "0021-add-v4l2m2m-encoder.patch" sunshinePackage.korriPatchNames;
assert (sunshineV4l2m2mPackage != null) == expectV4l2m2mPackage;
assert
  !expectV4l2m2mPackage
  || (
    sunshineV4l2m2mPackage.korriV4l2m2mEnabled
    && sunshineV4l2m2mPackage.korriBuildProfile == "aarch64-linux-v4l2m2m"
  );
pkgs.runCommand "sunshine-korri-v4l2m2m-check" { } ''
  ${pkgs.gawk}/bin/awk '
    /static const std::vector<encoder_t \*> encoders/ { registry = 1 }
    registry && /&v4l2m2m,/ { found = 1 }
    registry && /};/ { registry = 0 }
    END { exit !found }
  ' ${patchedSource}/src/video.cpp
  grep -F 'v4l2m2mPatch' ${approvedPatchesPath} >/dev/null
  grep -F 'v4l2m2mPatchSetSha256' ${approvedPatchesPath} >/dev/null
  grep -F 'ffmpegV4l2m2m' ${packagePath} >/dev/null
  grep -F 'korriV4l2m2mEnabled' ${packagePath} >/dev/null
  grep -F 'h264_v4l2m2m' ${patchPath} >/dev/null
  grep -F 'hevc_v4l2m2m' ${patchPath} >/dev/null
  grep -F 'extern encoder_t v4l2m2m' ${patchPath} >/dev/null
  grep -F 'av_image_copy2' ${ffmpegSource}/libavcodec/v4l2_buffers.c >/dev/null
  grep -F 'AV_OPT_TYPE_BOOL' ${ffmpegSource}/libavcodec/v4l2_m2m_enc.c >/dev/null
  grep -F 'MPEG_CID(PREPEND_SPSPPS_TO_IDR)' ${ffmpegSource}/libavcodec/v4l2_m2m_enc.c >/dev/null
  grep -F 'HEADER_MODE_JOINED_WITH_1ST_FRAME' ${ffmpegSource}/libavcodec/v4l2_m2m_enc.c >/dev/null
  grep -F '"repeat_headers"s, 1' ${patchedSource}/src/video.cpp >/dev/null
  grep -F 'FFmpeg PR #24328' ${ffmpegPatchPath} >/dev/null
  grep -F 'reviewedFfmpegCommit' ${ffmpegPackagePath} >/dev/null
  grep -F 'BUILD_FFMPEG_CBS_PATCHES=ON' ${ffmpegPackagePath} >/dev/null
  grep -F 'Rockchip RKMPP is a separate encoder profile' ${readmePath} >/dev/null
  ${
    if sunshineV4l2m2mPackage != null then
      ''
        test -x ${sunshineV4l2m2mPackage}/bin/sunshine
        provenance=${sunshineV4l2m2mPackage}/${sunshineV4l2m2mPackage.korriProvenanceRelativePath}
        grep -Fx 'build_profile=aarch64-linux-v4l2m2m' "$provenance" >/dev/null
        grep -Fx 'v4l2m2m_enabled=1' "$provenance" >/dev/null
      ''
    else
      ""
  }
  touch "$out"
''

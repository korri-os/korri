{
  sunshine,
  cudaSupport ? true,
  ffmpegV4l2m2m ? null,
  rkmppSupport ? false,
  ffmpegRkmpp ? null,
  rockchipMpp ? null,
  libdrm ? null,
}:

let
  baseSunshine = sunshine.override { inherit cudaSupport; };
in
baseSunshine.overrideAttrs (
  old:
  let
    approved = import ./approved-patches.nix;
    v4l2m2mEnabled = ffmpegV4l2m2m != null;
    baseBuildProfile = "${baseSunshine.stdenv.hostPlatform.system}-${
      if cudaSupport then "cuda" else "software"
    }";
    buildProfile =
      if rkmppSupport && v4l2m2mEnabled then
        "${baseSunshine.stdenv.hostPlatform.system}-rkmpp-v4l2m2m"
      else if rkmppSupport then
        "${baseSunshine.stdenv.hostPlatform.system}-rkmpp"
      else if v4l2m2mEnabled then
        "${baseSunshine.stdenv.hostPlatform.system}-v4l2m2m"
      else
        baseBuildProfile;
    # Every released encoder combination has an explicit approval key, even
    # when two profiles share the same unchanged upstream derivation.
    approvedBaseDerivations = approved.approvedBaseDerivationsByProfile.${buildProfile} or [ ];
    # Build-time identity only: emitting this path in runtime provenance retains
    # the entire checkout, even with its string context discarded. Runtime
    # provenance records the approved source hash instead.
    baseSunshineSource = builtins.unsafeDiscardStringContext (toString baseSunshine.src);
    baseSunshineSourceHash = baseSunshine.src.outputHash;
    baseSunshineDerivation = builtins.unsafeDiscardStringContext baseSunshine.drvPath;
    basePatches = old.patches or [ ];
    cudaEnabled =
      !(builtins.elem "-DSUNSHINE_ENABLE_CUDA:BOOL=FALSE" baseSunshine.cmakeFlags)
      && builtins.any (
        input: builtins.match ".*cuda_nvcc.*" (toString input) != null
      ) baseSunshine.nativeBuildInputs
      && builtins.any (
        input: builtins.match ".*(cuda_cudart|cuda-merged).*" (toString input) != null
      ) baseSunshine.buildInputs;
    matchingApprovedBaseDerivations = builtins.filter (
      candidate: candidate == baseSunshineDerivation
    ) approvedBaseDerivations;
    approvedBaseSunshineDerivation =
      if matchingApprovedBaseDerivations == [ ] then
        null
      else
        builtins.head matchingApprovedBaseDerivations;
    provenanceRelativePath = "share/korri/sunshine-korri/provenance";
    # Only RKMPP appends Sunshine patches; every other profile ships the base set.
    approvedPatchDefinitions =
      approved.patches ++ (if rkmppSupport then approved.rkmppPatches else [ ]);
    expectedPatchSetSha256 =
      if rkmppSupport then approved.rkmppPatchSetSha256 else approved.patchSetSha256;
    patchRecords = map (record: {
      inherit (record) name path sha256;
      actualName = builtins.baseNameOf record.path;
      actualSha256 = builtins.hashFile "sha256" record.path;
    }) approvedPatchDefinitions;
    patchesApproved = builtins.all (
      record: record.name == record.actualName && record.sha256 == record.actualSha256
    ) patchRecords;
    patchSetMaterial =
      builtins.concatStringsSep "\n" (map (record: "${record.name} ${record.sha256}") patchRecords)
      + "\n";
    actualPatchSetSha256 = builtins.hashString "sha256" patchSetMaterial;
    v4l2m2mPatchApproved = builtins.all (
      record:
      record.name == builtins.baseNameOf record.path
      && record.sha256 == builtins.hashFile "sha256" record.path
    ) approved.v4l2m2mPatches;
    v4l2m2mPatchSetMaterial =
      builtins.concatStringsSep "\n" (
        map (record: "${record.name} ${record.sha256}") approved.v4l2m2mPatches
      )
      + "\n";
    v4l2m2mPatchLines = builtins.concatStringsSep "\n" (
      map (record: "ffmpeg_patch=${record.name} sha256=${record.sha256}") approved.v4l2m2mPatches
    );
    v4l2m2mProvenance =
      if v4l2m2mEnabled then
        ''
          v4l2m2m_ffmpeg_patch_set_sha256=${approved.v4l2m2mPatchSetSha256}
          ${v4l2m2mPatchLines}
          reviewed_build_deps_commit=${approved.reviewedBuildDepsCommit}
          reviewed_build_deps_source_hash=${approved.reviewedBuildDepsSourceHash}
        ''
      else
        "";
    actualV4l2m2mPatchSetSha256 = builtins.hashString "sha256" v4l2m2mPatchSetMaterial;
    v4l2m2mFfmpegApproved =
      !v4l2m2mEnabled
      || (
        ffmpegV4l2m2m.ffmpegCommit == approved.reviewedFfmpegCommit
        && ffmpegV4l2m2m.libavcodecVersion == approved.reviewedLibavcodecVersion
        && ffmpegV4l2m2m.buildDepsRev == approved.reviewedBuildDepsCommit
        && ffmpegV4l2m2m.patchSetSha256 == approved.v4l2m2mPatchSetSha256
      );
    patchNames = map (record: record.name) patchRecords;
    patchLines = builtins.concatStringsSep "\n" (
      map (record: "patch=${record.name} sha256=${record.sha256}") patchRecords
    );
    provenance = ''
      format=1
      package=sunshine-korri
      build_profile=${buildProfile}
      cuda_enabled=${if cudaEnabled then "1" else "0"}
      base_sunshine_version=${approved.baseSunshineVersion}
      approved_base_sunshine_source_hash=${approved.approvedBaseSourceHash}
      base_sunshine_derivation=${baseSunshineDerivation}
      approved_base_sunshine_derivation=${approvedBaseSunshineDerivation}
      reviewed_libavcodec_version=${approved.reviewedLibavcodecVersion}
      reviewed_ffmpeg_commit=${approved.reviewedFfmpegCommit}
      reviewed_ffmpeg_source_hash=${approved.reviewedFfmpegSourceHash}
      v4l2m2m_enabled=${if v4l2m2mEnabled then "1" else "0"}
      ${v4l2m2mProvenance}reviewed_nvenc_api=${toString approved.reviewedNvencApiMajor}.${toString approved.reviewedNvencApiMinor}
      executable=bin/sunshine
      patch_set_sha256=${expectedPatchSetSha256}
      ${patchLines}
    '';
  in
  if old.version != approved.baseSunshineVersion then
    throw "sunshine-korri base version changed; review the approved patch set before building"
  else if baseSunshineSourceHash != approved.approvedBaseSourceHash then
    throw "sunshine-korri base source hash changed; review the approved source before building"
  else if cudaSupport != cudaEnabled then
    throw "sunshine-korri CUDA inputs do not match the selected ${buildProfile} profile"
  else if rkmppSupport && cudaSupport then
    throw "sunshine-korri RKMPP and CUDA profiles are mutually exclusive"
  else if
    rkmppSupport
    && v4l2m2mEnabled
    && (ffmpegRkmpp.outPath or null) != (ffmpegV4l2m2m.outPath or null)
  then
    throw "sunshine-korri combined RKMPP and V4L2 M2M support requires one reviewed FFmpeg bundle"
  else if rkmppSupport && baseSunshine.stdenv.hostPlatform.system != "aarch64-linux" then
    throw "sunshine-korri RKMPP is approved only for aarch64-linux"
  else if rkmppSupport && (ffmpegRkmpp == null || rockchipMpp == null || libdrm == null) then
    throw "sunshine-korri RKMPP requires the reviewed FFmpeg bundle, Rockchip MPP, and libdrm"
  else if v4l2m2mEnabled && cudaSupport then
    throw "sunshine-korri V4L2 M2M profile cannot include CUDA inputs"
  else if v4l2m2mEnabled && baseSunshine.stdenv.hostPlatform.system != "aarch64-linux" then
    throw "sunshine-korri V4L2 M2M profile is approved only for aarch64-linux"
  else if approvedBaseSunshineDerivation == null then
    throw "sunshine-korri ${buildProfile} base derivation changed; review the complete upstream recipe before building"
  else if basePatches != [ ] then
    throw "sunshine-korri base derivation carries unapproved patches"
  else if !patchesApproved then
    throw "sunshine-korri patch content differs from services/sunshine/approved-patches.nix"
  else if actualPatchSetSha256 != expectedPatchSetSha256 then
    throw "sunshine-korri ordered patch-set digest differs from services/sunshine/approved-patches.nix"
  else if !v4l2m2mPatchApproved then
    throw "sunshine-korri FFmpeg V4L2 M2M patch content differs from services/sunshine/approved-patches.nix"
  else if actualV4l2m2mPatchSetSha256 != approved.v4l2m2mPatchSetSha256 then
    throw "sunshine-korri FFmpeg V4L2 M2M patch-set digest differs from services/sunshine/approved-patches.nix"
  else if !v4l2m2mFfmpegApproved then
    throw "sunshine-korri V4L2 M2M FFmpeg provenance differs from the approved profile"
  else
    {
      pname = "sunshine-korri";
      version = "${approved.baseSunshineVersion}-korri";
      __intentionallyOverridingVersion = true;

      # Reject accidental output references to the build-only source checkout.
      disallowedReferences = (old.disallowedReferences or [ ]) ++ [ baseSunshine.src ];

      patches = map (record: record.path) approvedPatchDefinitions;

      cmakeFlags =
        (old.cmakeFlags or [ ])
        ++ (
          if rkmppSupport then
            [
              "-DFFMPEG_PREPARED_BINARIES=${ffmpegRkmpp}"
              "-DFFMPEG_PLATFORM_LIBRARIES=numa;va;va-drm;va-x11;X11;rockchip_mpp;drm"
            ]
          else if v4l2m2mEnabled then
            [ "-DFFMPEG_PREPARED_BINARIES=${ffmpegV4l2m2m}" ]
          else
            [ ]
        );

      buildInputs =
        (old.buildInputs or [ ])
        ++ (
          if rkmppSupport then
            [
              rockchipMpp
              libdrm
            ]
          else
            [ ]
        );

      postInstall = (old.postInstall or "") + ''
        install -d -m755 "$out/${builtins.dirOf provenanceRelativePath}"
        cat > "$out/${provenanceRelativePath}" <<'EOF'
        ${provenance}EOF
        chmod 0444 "$out/${provenanceRelativePath}"
      '';

      passthru = (old.passthru or { }) // {
        korriProvenanceRelativePath = provenanceRelativePath;
        korriPatchNames = patchNames;
        korriPatchSetSha256 = expectedPatchSetSha256;
        korriBuildProfile = buildProfile;
        korriBaseBuildProfile = baseBuildProfile;
        korriBaseSunshineVersion = approved.baseSunshineVersion;
        korriApprovedBaseSunshineSourceHash = approved.approvedBaseSourceHash;
        korriBaseSunshineSource = baseSunshineSource;
        korriBaseSunshineDerivation = baseSunshineDerivation;
        korriApprovedBaseSunshineDerivation = approvedBaseSunshineDerivation;
        korriReviewedLibavcodecVersion = approved.reviewedLibavcodecVersion;
        korriReviewedFfmpegCommit = approved.reviewedFfmpegCommit;
        korriReviewedFfmpegSourceHash = approved.reviewedFfmpegSourceHash;
        korriReviewedBuildDepsCommit = approved.reviewedBuildDepsCommit;
        korriReviewedBuildDepsSourceHash = approved.reviewedBuildDepsSourceHash;
        korriV4l2m2mEnabled = v4l2m2mEnabled;
        korriV4l2m2mPatchNames = map (record: record.name) approved.v4l2m2mPatches;
        korriV4l2m2mPatchSetSha256 = approved.v4l2m2mPatchSetSha256;
        korriRkmppEnabled = rkmppSupport;
        korriReviewedNvencApiMajor = approved.reviewedNvencApiMajor;
        korriReviewedNvencApiMinor = approved.reviewedNvencApiMinor;
        korriCudaEnabled = cudaEnabled;
      };

      meta = old.meta // {
        description = "Korri downstream Sunshine build with carried experimental patches";
      };
    }
)

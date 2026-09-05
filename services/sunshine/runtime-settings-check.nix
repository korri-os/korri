{
  pkgs,
  sunshinePackage,
  approvedPatchesPath,
  patchPaths,
  packagePath,
  readmePath,
}:

let
  lib = pkgs.lib;
  approved = import approvedPatchesPath;
  ffmpegSource = pkgs.fetchFromGitHub {
    owner = "FFmpeg";
    repo = "FFmpeg";
    rev = approved.reviewedFfmpegCommit;
    hash = approved.reviewedFfmpegSourceHash;
  };
  ffmpegLayoutSource = pkgs.runCommand "ffmpeg-nvenc-layout-source" { } ''
    cp -R ${ffmpegSource} "$out"
    chmod u+w "$out/libavcodec/hwconfig.h"
    substituteInPlace "$out/libavcodec/hwconfig.h" \
      --replace-fail 'AVCodecHWConfig public;' 'AVCodecHWConfig public_;'
  '';
  patchedSunshineSource = pkgs.applyPatches {
    name = "sunshine-korri-reviewed-source";
    src = pkgs.sunshine.src;
    patches = patchPaths;
  };
  contains = needle: haystack: lib.hasInfix needle haystack;
  check = message: assertion: { inherit message assertion; };
  patchSources = map builtins.readFile patchPaths;
  patchContains = needle: builtins.any (source: contains needle source) patchSources;
  packageSource = builtins.readFile packagePath;
  readme = builtins.readFile readmePath;
  actualPatchRecords = map (path: {
    name = builtins.baseNameOf path;
    sha256 = builtins.hashFile "sha256" path;
  }) patchPaths;
  approvedPatchRecords = map (record: {
    inherit (record) name sha256;
  }) approved.patches;
  patchManifestLines = builtins.concatStringsSep "\n" (
    map (record: "patch=${record.name} sha256=${record.sha256}") approved.patches
  );
  approvedProfileBaseDerivations =
    approved.approvedBaseDerivationsByProfile.${sunshinePackage.korriBuildProfile} or [ ];
  expectedBaseSunshine = pkgs.sunshine.override {
    cudaSupport = sunshinePackage.korriCudaEnabled;
  };
  expectedProvenance = ''
    format=1
    package=sunshine-korri
    build_profile=${sunshinePackage.korriBuildProfile}
    cuda_enabled=${if sunshinePackage.korriCudaEnabled then "1" else "0"}
    base_sunshine_version=${approved.baseSunshineVersion}
    approved_base_sunshine_source_hash=${approved.approvedBaseSourceHash}
    base_sunshine_source=${sunshinePackage.korriBaseSunshineSource}
    base_sunshine_derivation=${sunshinePackage.korriBaseSunshineDerivation}
    approved_base_sunshine_derivation=${sunshinePackage.korriBaseSunshineDerivation}
    reviewed_libavcodec_version=${approved.reviewedLibavcodecVersion}
    reviewed_ffmpeg_commit=${approved.reviewedFfmpegCommit}
    reviewed_ffmpeg_source_hash=${approved.reviewedFfmpegSourceHash}
    reviewed_nvenc_api=${toString approved.reviewedNvencApiMajor}.${toString approved.reviewedNvencApiMinor}
    executable=bin/sunshine
    patch_set_sha256=${approved.patchSetSha256}
    ${patchManifestLines}
  '';
  checks = [
    (check "the checked-in patch approval is exact" (
      actualPatchRecords == approvedPatchRecords
      && sunshinePackage.korriPatchNames == map (record: record.name) approved.patches
      && sunshinePackage.korriPatchSetSha256 == approved.patchSetSha256
      && sunshinePackage.korriBaseSunshineVersion == approved.baseSunshineVersion
      && sunshinePackage.korriApprovedBaseSunshineSourceHash == approved.approvedBaseSourceHash
      && builtins.elem sunshinePackage.korriBaseSunshineDerivation approvedProfileBaseDerivations
      && sunshinePackage.korriApprovedBaseSunshineDerivation == sunshinePackage.korriBaseSunshineDerivation
      && pkgs.sunshine.src.outputHash == approved.approvedBaseSourceHash
      && sunshinePackage.korriReviewedLibavcodecVersion == approved.reviewedLibavcodecVersion
      && sunshinePackage.korriReviewedFfmpegCommit == approved.reviewedFfmpegCommit
      && sunshinePackage.korriReviewedFfmpegSourceHash == approved.reviewedFfmpegSourceHash
      && sunshinePackage.korriReviewedNvencApiMajor == approved.reviewedNvencApiMajor
      && sunshinePackage.korriReviewedNvencApiMinor == approved.reviewedNvencApiMinor
    ))
    (check "Wayland CUDA capture uses the approved RAM transfer path" (
      builtins.elem "0017-use-wayland-ram-capture-for-cuda.patch" sunshinePackage.korriPatchNames
      && patchContains "NVIDIA headless wlroots buffers can carry modifiers"
      && patchContains "+    if (hwdevice_type == platf::mem_type_e::vaapi) {"
      && patchContains "DRM_FORMAT_BGR888"
      && patchContains "destination[0] = source[2];"
      && patchContains "destination[2] = source[0];"
    ))
    (check "Wayland BGR888 conversion uses the reviewed swscale path" (
      builtins.elem "0018-vectorize-wayland-bgr888-with-swscale.patch" sunshinePackage.korriPatchNames
      && patchContains "sws_getContext("
      && patchContains "AV_PIX_FMT_RGB24"
      && patchContains "AV_PIX_FMT_BGRA"
      && patchContains "sws_scale("
      && patchContains "source_linesize[0] = -source_linesize[0];"
    ))
    (check "CUDA Wayland conversion uses reusable pinned host images" (
      builtins.elem "0019-use-pinned-memory-for-cuda-capture.patch" sunshinePackage.korriPatchNames
      && patchContains "cudaMallocHost(&data, size)"
      && patchContains "cudaFreeHost(data)"
      && patchContains "img->cuda_pinned = img->data != nullptr;"
    ))
    (check "runtime settings packet IDs remain stable" (
      patchContains "RUNTIME_SETTINGS_REQUEST_PACKET = 0x5504"
      && patchContains "RUNTIME_SETTINGS_ACK_PACKET = 0x5505"
    ))
    (check "runtime settings operation IDs remain stable" (
      patchContains "RUNTIME_SETTINGS_OPERATION_QUERY_CAPABILITIES = 0"
      && patchContains "RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS = 1"
      && patchContains "RUNTIME_SETTINGS_OPERATION_SET_FPS = 2"
      && patchContains "RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION = 3"
    ))
    (check "runtime settings status IDs remain stable" (
      patchContains "RUNTIME_SETTINGS_STATUS_APPLIED = 0"
      && patchContains "RUNTIME_SETTINGS_STATUS_FAILED = 1"
      && patchContains "RUNTIME_SETTINGS_STATUS_INVALID = 2"
      && patchContains "RUNTIME_SETTINGS_STATUS_DISABLED = 3"
    ))
    (check "runtime settings reason IDs remain stable" (
      patchContains "RUNTIME_SETTINGS_REASON_NONE = 0"
      && patchContains "RUNTIME_SETTINGS_REASON_GATE_DISABLED = 1"
      && patchContains "RUNTIME_SETTINGS_REASON_INVALID_BOUNDS = 2"
      && patchContains "RUNTIME_SETTINGS_REASON_INVALID_PAYLOAD = 3"
      && patchContains "RUNTIME_SETTINGS_REASON_UNSUPPORTED_ENCODER = 4"
      && patchContains "RUNTIME_SETTINGS_REASON_UNSUPPORTED_BACKEND = 5"
      && patchContains "RUNTIME_SETTINGS_REASON_UNSUPPORTED_OPERATION = 6"
      && patchContains "RUNTIME_SETTINGS_REASON_APPLY_FAILED = 7"
      && patchContains "RUNTIME_SETTINGS_REASON_CONTROL_NOT_READY = 8"
      && patchContains "RUNTIME_SETTINGS_REASON_NO_ACK = 9"
      && patchContains "RUNTIME_SETTINGS_REASON_CONFLICT = 10"
      && patchContains "RUNTIME_SETTINGS_REASON_STALE_ACK = 11"
      && patchContains "RUNTIME_SETTINGS_REASON_STREAM_ENDED = 12"
      && patchContains "RUNTIME_SETTINGS_REASON_PROOF_GATED = 13"
    ))
    (check "the package does not apply obsolete diagnostic patches" (
      !(contains "0006-diagnose-vaapi-convert-sequence.patch" packageSource)
      && !(contains "0007-finish-vaapi-gl-convert-before-encode.patch" packageSource)
      && !(contains "0009-diagnose-avcodec-packet-content-after-resolution.patch" packageSource)
      && !(contains "0011-force-vaapi-vram-source-copy-before-convert.patch" packageSource)
    ))
    (check "runtime settings do not expose a pending success state" (
      !(patchContains "RUNTIME_SETTINGS_STATUS_ACCEPTED_PENDING")
      && !(contains "accepted/pending" readme)
      && !(patchContains "accepted=1")
    ))
    (check "queued requests do not imply final success" (patchContains "queued=1"))
    (check "the runtime settings gate accepts only the exact value 1" (
      patchContains "SUNSHINE_LIVE_SETTINGS_MVP"
      && patchContains ''std::string_view {enabled_value} == "1"sv''
    ))
    (check "requests and acknowledgements use typed queues" (
      patchContains "queue_t<video::runtime_bitrate_request_t> runtime_bitrate_events"
      && patchContains "queue_t<video::runtime_bitrate_ack_t> runtime_bitrate_ack_queue"
      && patchContains "mail->queue<runtime_bitrate_request_t>(mail::runtime_bitrate)"
      && patchContains "mail->queue<runtime_bitrate_ack_t>(mail::runtime_bitrate_ack)"
      && patchContains "boost::endian::little_uint16_at reason"
    ))
    (check "runtime bitrate has no unsafe AVCodec fallback" (
      !(patchContains "update_runtime_bitrate")
      && !(patchContains "avcodec runtime bitrate update")
      && !(patchContains "AV_OPT_SEARCH_CHILDREN")
    ))
    (check "runtime bitrate uses seamless VAAPI and NVENC paths without restart fallback" (
      patchContains "runtime_update_h264_vaapi_bitrate"
      && patchContains "VAAPI runtime bitrate params updated without encoder restart"
      && patchContains "runtime_update_h264_nvenc_bitrate"
      && patchContains "NVENC runtime bitrate reconfigured without encoder restart"
      && patchContains "seamless_backend="
      && patchContains "request_idr_frame"
      && !(patchContains "encoder restarted for runtime bitrate")
      && !(patchContains "disp->dummy_img(dummy_img.get())")
    ))
    (check "live H.264 selects only an advertised bitrate-capable VAAPI entrypoint" (
      patchContains ''std::getenv("SUNSHINE_LIVE_SETTINGS_MVP")''
      && patchContains "live_settings[0] == '1'"
      && patchContains "live_settings[1] == '\\0'"
      && patchContains "profile == VAProfileH264ConstrainedBaseline"
      && patchContains "profile == VAProfileH264Main"
      && patchContains "profile == VAProfileH264High"
      && patchContains "VAEntrypointEncSlice,"
      && patchContains "VAEntrypointEncSliceLP"
      && patchContains "VAConfigAttribRateControl"
      && patchContains "rc_attr.value != VA_ATTRIB_NOT_SUPPORTED"
      && patchContains "VA_RC_CBR | VA_RC_VBR | VA_RC_AVBR"
      && patchContains "Preserve Sunshine's upstream order unless the exact Korri gate found"
    ))
    (check "the private VAAPI and NVENC mirrors fail closed on exact libavcodec drift" (
      patchContains "#define KORRI_SUPPORTED_LIBAVCODEC_MAJOR 62"
      && patchContains "#define KORRI_SUPPORTED_LIBAVCODEC_VERSION AV_VERSION_INT(62, 11, 100)"
      && patchContains "LIBAVCODEC_VERSION_MAJOR != KORRI_SUPPORTED_LIBAVCODEC_MAJOR"
      && patchContains "LIBAVCODEC_VERSION_INT != KORRI_SUPPORTED_LIBAVCODEC_VERSION"
      && patchContains "must be reviewed for this exact FFmpeg libavcodec version"
      && patchContains "korri_nvenc_runtime_layout.h"
      && patchContains "korri_nvenc_context_t"
      && patchContains "KORRI_NVENC_MAX_REGISTERED_FRAMES = 64"
    ))
    (check "NVENC bitrate capability and completion are driver-backed and fail closed" (
      patchContains "runtime_h264_nvenc_bitrate_supported"
      && patchContains ''std::string_view {avctx->codec->name} != "h264_nvenc"sv''
      && patchContains "avctx->pix_fmt != AV_PIX_FMT_CUDA"
      && patchContains "support_dyn_bitrate > 0"
      && patchContains "NV_ENC_PARAMS_RC_CONSTQP"
      && patchContains "nvEncReconfigureEncoder != nullptr"
      && patchContains "auto updated_config = nvenc->encode_config"
      && patchContains "updated_init.encodeConfig = &updated_config"
      && patchContains "params.resetEncoder = 1"
      && patchContains "params.forceIDR = 1"
      && patchContains "result != NV_ENC_SUCCESS"
      && patchContains "nvenc->encode_config.rcParams = updated_config.rcParams"
      && patchContains "scaled_buffer"
      && patchContains "RUNTIME_NVENC_BITRATE_MIN_INTERVAL"
      && patchContains "nvenc_rate_limited"
      && patchContains "nvenc_attempt"
      && patchContains "last_runtime_nvenc_bitrate = bitrate_completed"
      && patchContains "RUNTIME_NVENC_BITRATE_SLOW_CALL"
      && patchContains "runtime_nvenc_bitrate_blocked"
      && patchContains "NVENC runtime bitrate disabled for session after failed or slow driver call"
      && patchContains "RUNTIME_SETTINGS_REASON_CONFLICT"
    ))
    (check "every fresh NVENC encoder resets the circuit before capability publication" (
      patchContains ''
+                    if (runtime_settings_supports_nvenc_h264(encoder, updated_config)) {
+                      last_runtime_nvenc_bitrate = std::chrono::steady_clock::time_point {};
+                      runtime_nvenc_bitrate_blocked = false;
+                    }
+                    runtime_settings_supports_encoder_restart->raise(runtime_settings_supports_h264_dynamic(encoder, updated_config));
+                    runtime_settings_supports_bitrate->raise(runtime_h264_bitrate_supported(session.get()));
''
      && patchContains ''
+    if (raise_runtime_state) {
+      if (runtime_settings_supports_nvenc_h264(encoder, ctx.config)) {
+        ctx.last_runtime_nvenc_bitrate = std::chrono::steady_clock::time_point {};
+        ctx.runtime_nvenc_bitrate_blocked = false;
+      }
+      ctx.runtime_settings_supports_encoder_restart->raise(runtime_settings_supports_h264_dynamic(encoder, ctx.config));
+      ctx.runtime_settings_supports_bitrate->raise(runtime_h264_bitrate_supported(session.get()));
+    }
''
      && patchContains ''
+                    if (runtime_settings_supports_nvenc_h264(encoder, ctx->config)) {
+                      ctx->last_runtime_nvenc_bitrate = std::chrono::steady_clock::time_point {};
+                      ctx->runtime_nvenc_bitrate_blocked = false;
+                    }
+                    ctx->runtime_settings_supports_encoder_restart->raise(runtime_settings_supports_h264_dynamic(encoder, ctx->config));
+                    ctx->runtime_settings_supports_bitrate->raise(runtime_h264_bitrate_supported(pos->session.get()));
''
    ))
    (check "explicit strict encoder selection refuses automatic fallback" (
      patchContains ''std::getenv("SUNSHINE_STRICT_ENCODER")''
      && patchContains "value[0] == '1' && value[1] == '\\0'"
      && patchContains "Strict encoder selection refuses automatic fallback"
      && patchContains ''
+        if (strict_encoder_selection_enabled()) {
+          BOOST_LOG(error) << "Strict encoder selection refuses automatic fallback"sv;
+          return -1;
+        }
''
    ))
    (check "runtime FPS and resolution support is limited to H.264 VAAPI or NVENC" (
      patchContains "runtime_settings_supports_h264_dynamic"
      && patchContains "runtime_settings_supports_vaapi_h264"
      && patchContains "runtime_settings_supports_nvenc_h264"
      && patchContains ''encoder.name == "vaapi"sv''
      && patchContains ''encoder.codec_from_config(config).name == "h264_vaapi"''
      && patchContains ''encoder.name == "nvenc"sv''
      && patchContains ''encoder.codec_from_config(config).name == "h264_nvenc"''
      && patchContains "runtime FPS unsupported"
    ))
    (check "runtime FPS uses experimental frame pacing" (
      contains "supports operation `2`: set effective stream FPS" readme
      && patchContains "runtime FPS"
      && patchContains "applied_fps"
      && patchContains "runtime_fps_interval"
    ))
    (check "runtime FPS rejects impossible upshifts before queueing" (
      patchContains "operation == video::RUNTIME_SETTINGS_OPERATION_SET_FPS && requested_value >"
      && patchContains "rejection_status = video::RUNTIME_SETTINGS_STATUS_INVALID"
    ))
    (check "runtime FPS failures preserve the current applied value" (
      patchContains "live-settings-mvp: runtime FPS unsupported"
      && patchContains "RUNTIME_SETTINGS_STATUS_FAILED"
      && patchContains "runtime_bitrate_ack_t {request->request_id, RUNTIME_SETTINGS_OPERATION_SET_FPS, status, applied_fps, 0, 0, reason}"
    ))
    (check "capability acknowledgements expose complete active-session facts" (
      patchContains "control_runtime_settings_capability_ack_t"
      && patchContains "supported_operations"
      && patchContains "proof_gated_operations"
      && patchContains "min_bitrate_kbps"
      && patchContains "max_bitrate_kbps"
      && patchContains "max_fps"
      && patchContains "current_bitrate_kbps"
      && patchContains "current_fps"
      && patchContains "current_width"
      && patchContains "current_height"
      && patchContains "launch_bitrate_kbps"
      && patchContains "launch_fps"
      && patchContains "launch_width"
      && patchContains "launch_height"
    ))
    (check "capability support uses active per-session encoder facts" (
      patchContains "MAIL(runtime_settings_supports_encoder_restart)"
      && patchContains "MAIL(runtime_settings_supports_bitrate)"
      && patchContains "session->control.runtime_settings_supports_encoder_restart"
      && patchContains "session->control.runtime_settings_supports_bitrate"
      && patchContains "session->control.runtime_settings_encoder_restart_supported"
      && patchContains "session->control.runtime_settings_bitrate_supported"
      && patchContains "runtime_settings_supports_encoder_restart->raise(runtime_settings_supports_h264_dynamic"
      && patchContains "runtime_settings_supports_bitrate->raise(runtime_h264_bitrate_supported(session.get()))"
      && patchContains "ctx->runtime_settings_supports_bitrate->raise(runtime_h264_bitrate_supported(pos->session.get()))"
      && patchContains "runtime_settings_supports_encoder_restart->raise(runtime_settings_supports_h264_dynamic(encoder, updated_config))"
      && patchContains "ctx->runtime_settings_supports_encoder_restart->raise(runtime_settings_supports_h264_dynamic(encoder, ctx->config))"
      && patchContains "runtime_settings_supports_encoder_restart->raise(false)"
      && patchContains "ctx->runtime_settings_supports_encoder_restart->raise(false)"
      && patchContains "runtime_settings_supports_bitrate->raise(false)"
      && patchContains "ctx->runtime_settings_supports_bitrate->raise(false)"
      && patchContains "auto runtime_supported = enabled && session->control.runtime_settings_encoder_restart_supported"
      && patchContains "auto runtime_bitrate_supported = runtime_supported && session->control.runtime_settings_bitrate_supported"
    ))
    (check "capability and mutation bitrate bounds remain conservative" (
      patchContains "RUNTIME_SETTINGS_MIN_BITRATE_KBPS = 500"
      && patchContains "RUNTIME_SETTINGS_MAX_BITRATE_KBPS = 150000"
      && patchContains "plaintext.min_bitrate_kbps = runtime_bitrate_supported ? RUNTIME_SETTINGS_MIN_BITRATE_KBPS : 0"
      && patchContains "plaintext.max_bitrate_kbps = runtime_bitrate_supported ? RUNTIME_SETTINGS_MAX_BITRATE_KBPS : 0"
      && patchContains "requested_value < RUNTIME_SETTINGS_MIN_BITRATE_KBPS"
      && patchContains "requested_value > RUNTIME_SETTINGS_MAX_BITRATE_KBPS"
      && patchContains "if (runtime_bitrate_supported)"
      && patchContains "supported_operations |= 1u << video::RUNTIME_SETTINGS_OPERATION_SET_BITRATE_KBPS"
      && patchContains "supported_operations |= 1u << video::RUNTIME_SETTINGS_OPERATION_SET_FPS"
    ))
    (check "validated sessions advertise runtime resolution as operation 3" (
      patchContains "proof_gated_operations"
      && patchContains "supported_operations |= 1u << video::RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION"
      && !(patchContains "proof_gated_operations |= 1u << video::RUNTIME_SETTINGS_OPERATION_SET_RESOLUTION")
      && contains "Runtime resolution is a normal runtime-settings operation for the validated Korri profile" readme
    ))
    (check "runtime resolution rejects unsafe bounds and aspect changes" (
      patchContains "requested_width == 0 || requested_height == 0"
      && patchContains "requested_width > launch_width"
      && patchContains "requested_height > launch_height"
      && patchContains "aspect_tolerance"
      && patchContains "aspect_abs_delta <= aspect_tolerance"
      && patchContains "!same_aspect"
    ))
    (check "host requests are exact before logging, scheduling, or queueing" (
      patchContains "request_id == 0 || reserved != 0 || payload.size() != expected_payload_size"
      && patchContains "expected_payload_size = sizeof(control_runtime_settings_request_prefix_t)"
      && patchContains "expected_payload_size = sizeof(control_runtime_settings_resolution_request_t)"
      && patchContains "expected_payload_size = sizeof(control_runtime_settings_request_t)"
      && patchContains "runtime_settings_capability_pending"
      && patchContains "runtime_settings_capability_due = std::chrono::steady_clock::now() + 100ms"
      && patchContains "now >= session->control.runtime_settings_capability_due"
      && patchContains "send_runtime_settings_capability_ack(session, request_id, enabled)"
      && !(patchContains "steady_timer")
      && !(patchContains "async_wait")
      && !(patchContains ").detach()")
    ))
    (check "runtime resolution uses explicit width and height fields" (
      patchContains "control_runtime_settings_request_prefix_t"
      && patchContains "boost::endian::little_uint32_at width"
      && patchContains "boost::endian::little_uint32_at height"
      && patchContains "applied_width"
      && patchContains "applied_height"
    ))
    (check "launch baselines remain separate from current applied values" (
      patchContains "runtime_settings_launch_bitrate_kbps"
      && patchContains "runtime_settings_applied_bitrate_kbps"
      && patchContains "runtime_settings_launch_fps"
      && patchContains "runtime_settings_applied_fps"
      && patchContains "runtime_settings_launch_width"
      && patchContains "runtime_settings_applied_width"
      && patchContains "runtime_settings_launch_height"
      && patchContains "runtime_settings_applied_height"
      && patchContains "plaintext.launch_bitrate_kbps"
      && patchContains "plaintext.current_bitrate_kbps"
    ))
    (check "successful acknowledgements update applied values only" (
      patchContains "session->control.runtime_settings_applied_bitrate_kbps = ack.applied_value"
      && patchContains "session->control.runtime_settings_applied_fps = ack.applied_value"
      && patchContains "session->control.runtime_settings_applied_width = ack.applied_width"
      && patchContains "session->control.runtime_settings_applied_height = ack.applied_height"
      && patchContains "session->control.runtime_settings_launch_bitrate_kbps = (std::uint32_t) session->config.monitor.bitrate"
    ))
    (check "runtime resolution failures preserve current dimensions" (
      patchContains "auto current_applied_width = session->control.runtime_settings_applied_width"
      && patchContains "auto current_applied_height = session->control.runtime_settings_applied_height"
      && patchContains "send_runtime_settings_ack(session, video::runtime_bitrate_ack_t {request_id, operation, rejection_status, 0, current_applied_width, current_applied_height, rejection_reason})"
      && patchContains "runtime resolution unsupported encoder"
      && patchContains "ack.status == video::RUNTIME_SETTINGS_STATUS_APPLIED"
    ))
    (check "touch mapping refreshes only after resolution apply" (
      patchContains "runtime resolution"
      && patchContains "touch_port"
      && patchContains "make_port"
      && patchContains "ack.status == video::RUNTIME_SETTINGS_STATUS_APPLIED"
      && patchContains "applied_width"
      && patchContains "applied_height"
    ))
    (check "runtime resolution refreshes capture state after apply" (
      patchContains "config_t &config" && patchContains "runtime_reinit_event"
    ))
    (check "the VAAPI destructor skip remains VAAPI-only for the replacement pair" (
      patchContains "disable_destructor_flush_after_runtime_vaapi_replacement"
      && patchContains "runtime VAAPI replacement: destructor flush disabled for replacement pair"
      && patchContains "if (runtime_settings_supports_vaapi_h264(encoder, config))"
      && !(patchContains "disable_flush_on_destroy")
    ))
    (check "package provenance is anchored and complete" (
      sunshinePackage.korriProvenanceRelativePath == "share/korri/sunshine-korri/provenance"
      && sunshinePackage.korriApprovedBaseSunshineSourceHash == approved.approvedBaseSourceHash
      && pkgs.sunshine.src.outputHash == approved.approvedBaseSourceHash
      &&
        sunshinePackage.korriBaseSunshineSource
        == builtins.unsafeDiscardStringContext (toString pkgs.sunshine.src)
      &&
        sunshinePackage.korriBaseSunshineDerivation
        == builtins.unsafeDiscardStringContext expectedBaseSunshine.drvPath
      && builtins.elem sunshinePackage.korriBaseSunshineDerivation approvedProfileBaseDerivations
      && sunshinePackage.korriApprovedBaseSunshineDerivation == sunshinePackage.korriBaseSunshineDerivation
      && contains "Package provenance" readme
    ))
  ];
  failures = builtins.filter (candidate: !candidate.assertion) checks;
in
if failures != [ ] then
  throw "Korri Sunshine runtime settings check failed:\n${
    lib.concatMapStringsSep "\n" (failure: "- ${failure.message}") failures
  }"
else
  pkgs.runCommand "sunshine-korri-runtime-settings-check"
    { nativeBuildInputs = [ pkgs.gcc pkgs.boost ]; } ''
    test -x ${sunshinePackage}/bin/sunshine
    c++ -std=c++20 -O2 -Wall -Wextra -Werror -pthread \
      ${./test-runtime-settings-host.cpp} -o host-runtime-settings-test
    ./host-runtime-settings-test

    bundled=${pkgs.sunshine.src}/third-party/build-deps/dist/Linux-x86_64/include
    grep -Fx '#define FFMPEG_VERSION "61c5040"' "$bundled/libavutil/ffversion.h" >/dev/null
    grep -Fx '#define NVENCAPI_MAJOR_VERSION 12' "$bundled/ffnvcodec/nvEncodeAPI.h" >/dev/null
    grep -Fx '#define NVENCAPI_MINOR_VERSION 0' "$bundled/ffnvcodec/nvEncodeAPI.h" >/dev/null
    c++ -std=c++20 -O2 -Wall -Wextra -Werror \
      -I"$bundled" \
      -I${ffmpegLayoutSource} \
      -I${ffmpegLayoutSource}/libavcodec \
      -I${patchedSunshineSource}/src \
      ${./test-nvenc-layout.cpp} -o nvenc-layout-test
    ./nvenc-layout-test

    finalVideo=${patchedSunshineSource}/src/video.cpp
    grep -F 'runtime_settings_supports_h264_dynamic' "$finalVideo" >/dev/null
    grep -F 'NVENC runtime bitrate reconfigured without encoder restart' "$finalVideo" >/dev/null
    grep -F 'runtime_settings_supports_bitrate->raise(runtime_h264_bitrate_supported(session.get()))' "$finalVideo" >/dev/null
    grep -F 'ctx.runtime_settings_supports_bitrate->raise(runtime_h264_bitrate_supported(session.get()))' "$finalVideo" >/dev/null
    test "$(grep -Fc 'ctx->runtime_settings_supports_bitrate->raise(runtime_h264_bitrate_supported(pos->session.get()))' "$finalVideo")" = 1
    grep -F 'RUNTIME_NVENC_BITRATE_MIN_INTERVAL' "$finalVideo" >/dev/null
    grep -F 'if (runtime_settings_supports_vaapi_h264(encoder, config))' "$finalVideo" >/dev/null
    if grep -F 'seamless_vaapi=1' "$finalVideo" >/dev/null; then
      echo 'final applied source retained a superseded VAAPI-only success marker' >&2
      exit 1
    fi

    provenance=${sunshinePackage}/${sunshinePackage.korriProvenanceRelativePath}
    test -f "$provenance"
    test "$(stat -c '%a' "$provenance")" = 444

    cat > expected-provenance <<'EOF'
    ${expectedProvenance}EOF
    cmp expected-provenance "$provenance"

    mkdir -p "$out"
    printf '%s\n' '${approved.patchSetSha256}' > "$out/patch-set-sha256"
  ''

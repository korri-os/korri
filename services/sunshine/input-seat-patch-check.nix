{
  pkgs,
  sunshinePackage,
  approvedPatchesPath,
  nonblockingTestPath,
  patchPath,
  packagePath,
  readmePath,
}:

let
  lib = pkgs.lib;
  approved = import approvedPatchesPath;
  contains = needle: haystack: lib.hasInfix needle haystack;
  check = message: assertion: { inherit message assertion; };
  patch = builtins.readFile patchPath;
  packageSource = builtins.readFile packagePath;
  readme = builtins.readFile readmePath;
  patchName = builtins.baseNameOf patchPath;
  approvedPatch = lib.findFirst (record: record.name == patchName) null approved.patches;
  patchSha256 = builtins.hashFile "sha256" patchPath;
  expectedPatchSetSha256 =
    if sunshinePackage.korriRkmppEnabled or false then
      approved.rkmppPatchSetSha256
    else
      approved.patchSetSha256;
  checks = [
    (check "the approved input-seat patch is exact and applied" (
      approvedPatch != null
      && patchSha256 == approvedPatch.sha256
      && contains "approved-patches.nix" packageSource
      && builtins.elem patchName sunshinePackage.korriPatchNames
      && sunshinePackage.korriPatchSetSha256 == expectedPatchSetSha256
      && sunshinePackage.korriApprovedBaseSunshineSourceHash == approved.approvedBaseSourceHash
      && pkgs.sunshine.src.outputHash == approved.approvedBaseSourceHash
    ))
    (check "the sidecar path is fixed below one stable runtime directory" (
      contains "KORRI_INPUT_SEAT_RUNTIME_DIR" patch
      && contains ''KORRI_INPUT_SEAT_SIDECAR_NAME = "sunshine-active-launch.json"sv'' patch
      && contains "::open(runtime_dir, O_RDONLY | O_DIRECTORY | O_CLOEXEC | O_NOFOLLOW)" patch
      && contains "::openat(dir_fd, KORRI_INPUT_SEAT_SIDECAR_NAME.data(), O_RDONLY | O_CLOEXEC | O_NOFOLLOW)" patch
      && !(contains "KORRI_INPUT_SEAT_LAUNCH_ID" patch)
    ))
    (check "the authority sidecar has an exact small size and file boundary" (
      contains "KORRI_INPUT_SEAT_MAX_SIDECAR_BYTES = 4096" patch
      && contains "std::array<char, KORRI_INPUT_SEAT_MAX_SIDECAR_BYTES + 1>" patch
      && contains "S_ISREG(sidecar_stat.st_mode)" patch
      && contains "sidecar_stat.st_uid == 0" patch
      && contains "sidecar_stat.st_gid == ::getegid()" patch
      && contains "sidecar_stat.st_mode & S_IRGRP" patch
      && contains "S_IWGRP | S_IXGRP | S_IRWXO" patch
      && !(contains "sidecar_stat.st_uid == ::geteuid()" patch)
      && contains "sidecar_stat.st_size > 0" patch
      && contains "used > KORRI_INPUT_SEAT_MAX_SIDECAR_BYTES" patch
    ))
    (check "the sidecar parser accepts only the exact authority schema" (
      contains "nlohmann::json::parse" patch
      && contains "document.size() != 3" patch
      && contains ''document.contains("launchId")'' patch
      && contains ''document["launchId"].is_string()'' patch
      && contains ''document.contains("generation")'' patch
      && contains ''document["generation"].is_number_integer()'' patch
      && contains "signed_generation < 0" patch
      && contains ''document.contains("mirrorToken")'' patch
      && contains ''document["mirrorToken"].is_string()'' patch
      && contains "KORRI_INPUT_SEAT_MAX_LAUNCH_ID_BYTES = 128" patch
      && contains "KORRI_INPUT_SEAT_MAX_MIRROR_TOKEN_BYTES = 512" patch
    ))
    (check "the mirror uses bounded nonblocking close-on-exec Unix sockets" (
      contains "KORRI_INPUT_SEAT_MAX_FRAME_BYTES = 2048" patch
      && contains "AF_UNIX" patch
      && contains "SOCK_SEQPACKET | SOCK_CLOEXEC | SOCK_NONBLOCK" patch
      && !(contains "SOCK_STREAM | SOCK_CLOEXEC | SOCK_NONBLOCK" patch)
      && contains "MSG_NOSIGNAL | MSG_DONTWAIT" patch
      && contains "connect_error != EINPROGRESS" patch
      && contains "connect_error != EAGAIN" patch
      && contains "result != static_cast<ssize_t>(frame.size())" patch
      && !(contains "while (written < frame.size())" patch)
    ))
    (check "backpressure drops one complete message without waiting" (
      contains "::close(fd);" patch
      && contains "connect_error = errno" patch
      && contains "auto incomplete_send = result != static_cast<ssize_t>(frame.size())" patch
      && contains "if (incomplete_send" patch
      && contains "failed to write complete input seat mirror message" patch
    ))
    (check "frames keep the legacy bounded token-envelope schema" (
      contains ''frame += "{\"mirrorToken\":\""'' patch
      && contains ''frame += "\",\"frame\":"'' patch
      && contains ''korri_input_seat_frame_prefix("source-connected"sv'' patch
      && contains ''korri_input_seat_frame_prefix("source-state"sv'' patch
      && contains ''korri_input_seat_frame_prefix("source-disconnected"sv'' patch
      && contains "controllerNumber" patch
      && contains "leftTrigger" patch
      && contains "rightTrigger" patch
      && contains "leftStickX" patch
      && contains "leftStickY" patch
      && contains "rightStickX" patch
      && contains "rightStickY" patch
    ))
    (check "an active receiver replaces Sunshine native gamepad allocation" (
      contains "bool korri_input_seat_emit_json" patch
      && contains "bool korri_input_seat_receiver_configured()" patch
      && contains "if (!korri_input_seat_receiver_configured())" patch
      && contains "if (korri_input_seat_emit_source_connected(packet->controllerNumber))" patch
      && contains "if (korri_input_seat_emit_source_state(packet))" patch
      && contains "korri_input_seat_emit_source_disconnected" patch
      && contains "free_gamepad(platf_input, gamepad.id)" patch
      && contains "does not create a second gamepad" patch
    ))
    (check "the mirror forwards no non-controller packet domain" (
      !(contains "passthrough(PNV_KEYBOARD_PACKET" patch)
      && !(contains "passthrough(PNV_REL_MOUSE_MOVE_PACKET" patch)
      && !(contains "passthrough(PNV_UNICODE_PACKET" patch)
    ))
    (check "authority and frame contents never enter logs" (
      !(contains "BOOST_LOG(warning) << active." patch)
      && !(contains "BOOST_LOG(debug) << active." patch)
      && !(contains "BOOST_LOG(warning) << frame" patch)
      && !(contains "BOOST_LOG(debug) << frame" patch)
      && !(contains "BOOST_LOG(warning) << document" patch)
      && !(contains "BOOST_LOG(debug) << document" patch)
      && !(contains "BOOST_LOG(warning) << buffer" patch)
      && !(contains "BOOST_LOG(debug) << buffer" patch)
    ))
    (check "the hardened delta and protected receiver activation are documented" (
      contains "Input-seat event mirror patch" readme
      && contains "hardened from the legacy patch" readme
      && contains "openat" readme
      && contains "SOCK_SEQPACKET" readme
      && contains "Sunshine-source transport check" readme
      && contains "korri-input-seat-receiver" readme
      && contains "services.korriLinuxHost.sunshine.inputSeats.enable" readme
      && contains "remains inert" readme
      && contains "Physical device acceptance remains separate" readme
    ))
  ];
  failures = builtins.filter (candidate: !candidate.assertion) checks;
in
if failures != [ ] then
  throw "Korri Sunshine input-seat patch check failed:\n${
    lib.concatMapStringsSep "\n" (failure: "- ${failure.message}") failures
  }"
else
  pkgs.runCommand "sunshine-korri-input-seat-patch-check"
    {
      nativeBuildInputs = [ pkgs.python3 ];
    }
    ''
      test -x ${sunshinePackage}/bin/sunshine

      provenance=${sunshinePackage}/${sunshinePackage.korriProvenanceRelativePath}
      test -f "$provenance"
      grep -Fx 'approved_base_sunshine_source_hash=${approved.approvedBaseSourceHash}' "$provenance" >/dev/null
      grep -Fx 'patch=${patchName} sha256=${approvedPatch.sha256}' "$provenance" >/dev/null
      grep -Fx 'patch_set_sha256=${expectedPatchSetSha256}' "$provenance" >/dev/null

      # This checks the declared AF_UNIX SOCK_SEQPACKET transport model under
      # backpressure. The package build above proves that patch 0015 compiles;
      # final device validation proves the running Sunshine behavior.
      python3 ${nonblockingTestPath}

      mkdir -p "$out"
      printf '%s\n' '${approvedPatch.sha256}' > "$out/input-seat-patch-sha256"
    ''

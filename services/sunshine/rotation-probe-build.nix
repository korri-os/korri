# Off-device experiment only. Never present this derivation as an approved release.
{
  system ? "x86_64-linux",
  forceOff ? false,
}:
let
  flake = builtins.getFlake ("path:" + toString ../..);
  packageName = if system == "aarch64-linux" then "sunshine-korri-v4l2m2m" else "sunshine-korri";
  original = flake.packages.${system}.${packageName};
  probeHash = builtins.hashFile "sha256" ./rotation-probe.patch;
in
assert builtins.elem system [ "x86_64-linux" "aarch64-linux" ];
original.overrideAttrs (old: {
  pname = "sunshine-rotation-experiment";
  version = "${original.version}-probe-${if forceOff then "off" else "on"}";
  __intentionallyOverridingVersion = true;
  patches = old.patches ++ [ ./rotation-probe.patch ];
  cmakeFlags = old.cmakeFlags ++ [ "-DCMAKE_CXX_FLAGS=-DSUNSHINE_CAPTURE_ROTATION_PROBE" ];
  env = (old.env or { }) // (if forceOff then {
    NIX_CFLAGS_COMPILE = (old.env.NIX_CFLAGS_COMPILE or "") + " -DSUNSHINE_CAPTURE_ROTATION_FORCE_OFF";
  } else { });
  postInstall = (old.postInstall or "") + ''
    chmod u+w "$out/share/korri/sunshine-korri/provenance"
    substituteInPlace "$out/share/korri/sunshine-korri/provenance" \
      --replace-fail 'package=sunshine-korri' 'package=sunshine-rotation-experiment' \
      --replace-fail 'patch_set_sha256=' 'approved_parent_patch_set_sha256='
    printf '%s\n' 'experimental_rotation_probe=1' \
      'experimental_rotation_patch_sha256=${probeHash}' \
      'experimental_rotation_force_off=${if forceOff then "1" else "0"}' \
      'experimental_parent_profile=${original.korriBuildProfile}' \
      >> "$out/share/korri/sunshine-korri/provenance"
    chmod 0444 "$out/share/korri/sunshine-korri/provenance"
  '';
})

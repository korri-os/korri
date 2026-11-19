# Linux kernel for the AYN Thor: the Odin 2 Portal kernel plus ROCKNIX's
# DPU resource-cleanup series, which a two-display board needs.
#
# The Portal kernel leaves 0000-version-0010-msm-resource-cleanup.patch out
# (../../odin2portal/kernel/default.nix), because the copy at the pinned rev
# 1178bc22 hung the Portal at DPU start. That copy writes through an
# uninitialised pointer: patch 1/7 sets `cstate->num_mixers` before `cstate`
# is assigned in dpu_crtc_assign_resources(). ROCKNIX fixed it in 9416eec2c2
# ("kernel: msm dp resource cleanup patch fix uninit pointer", 2026-10-06);
# patches 2/7 to 7/7 are unchanged.
#
# ./patches/0000-version-0010-msm-resource-cleanup.patch is that fixed file,
# unchanged, from ROCKNIX distribution origin/next f8c1832749ad (2026-11-18),
# projects/ROCKNIX/packages/linux/patches/7.2/0010-msm-resource-cleanup.patch
# (sha256 851e55cd4f59...8de8a). Every ROCKNIX SM8550 kernel, including the
# one that drove both Thor screens for legacy, applies this series.
#
# Why the Thor needs it: the series stops the DPU encoder from cleaning up
# the layer mixers and pingpongs that are assigned at disable time instead of
# the ones it used (patches 3/7, 5/7, 6/7). With two displays, the bottom
# screen's CRTC going off can then clear the top screen's blend stages. On
# the Thor that matches top-screen (DSI-2, command mode) page flips stalling
# in dpu_encoder_phys_cmd wait_for_idle while DSI-1 goes on or off. This is
# inferred, not yet proven on hardware.
#
# Hardware (2026-11-18): the series alone does not stop that stall; its
# trigger is Plymouth lighting DSI-1 at boot (README.md, "Root cause"). The
# series stays because every ROCKNIX SM8550 kernel carries it.
#
# ROCKNIX applies the series after the mainline patches and before the device
# patches; it is inserted at that point here.
{
  lib,
  odinKernel,
  # linuxPackagesFor re-invokes this function through `override`.
  features ? { },
  ...
}:

let
  name = "0000-version-0010-msm-resource-cleanup.patch";
  patch = ./patches + "/${name}";
  sha256 = "851e55cd4f594bee9458c8b395c982fcf458d55bd12020db094cca1bff62de8a";
in
assert builtins.hashFile "sha256" patch == sha256;
(odinKernel.override { inherit features; }).overrideAttrs (previous: {
  patches =
    let
      split = lib.partition (p: baseNameOf p < name) previous.patches;
    in
    assert !lib.any (p: baseNameOf p == name) previous.patches;
    split.right ++ [ patch ] ++ split.wrong;

  passthru = (previous.passthru or { }) // {
    dpuResourceCleanupPatch = name;
  };
})

# GPU and media userspace for the RG353M.
#
# The SoC carries a Mali-G52 MP1 (one shader core, 200-800 MHz) driven by
# Panfrost, plus three separate fixed-function blocks that matter more than 3D
# for streaming work: the Hantro decoder on /dev/video1, the Hantro encoder on
# /dev/video2, and the RGA 2D scaler/rotator on /dev/video0.
#
# Without hardware.graphics the image ships no DRI driver at all, so nothing
# can render. That is the first and largest win; clock tuning is worth far
# less and is measured separately.
{ pkgs, ... }:

{
  hardware.graphics = {
    enable = true;
    extraPackages = with pkgs; [
      # Panfrost lives in mesa; VA-API for the Hantro blocks goes through the
      # v4l2 request backend rather than a vendor driver.
      libva
      libva-vdpau-driver
    ];
  };

  environment.systemPackages = with pkgs; [
    # Measurement before tuning. glmark2 has a DRM/KMS backend, so it runs on
    # the panel with no display server.
    glmark2
    mesa-demos
    # Inspect and exercise the decoder, encoder, and RGA.
    v4l-utils
    libva-utils
  ];

  # Panfrost needs no firmware, but the VPU and RGA are exposed through V4L2
  # nodes owned by the video group.
  users.users.root.extraGroups = [ "video" ];
}

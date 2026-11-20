# AYN Thor hardware facts: screens, touch, controls and USB gadget for the
# shared Korri product. First slice: the top screen only. The bottom screen
# is left dark and its touch controller is ignored. README.md lists which
# fact comes from where and which still need the device.
{
  lib,
  pkgs,
  thorInputplumberData,
  ...
}:

let
  # Top screen. Observed on the Thor under ROCKNIX (nix-on-rocks
  # guest/modules/chipsets/sm8550/default.nix, validated 2026-05-08; legacy
  # products.nix homeOutput = "DSI-2"): DSI-2 is the top panel and transform
  # 90 shows it in landscape. The pinned DTS agrees on the panel: the top
  # (1080x1920) is the chipone,icna3520 on mdss_dsi1, the bottom (1080x1240)
  # the ch13726a,thor on mdss_dsi0. The mode is the first (120 Hz) mode of
  # icna3520_modes in kernel patch 0057_Chipone-ICNA35XX-panel.patch.
  top = "DSI-2";
  # Bottom screen, the same sources.
  bottom = "DSI-1";
in
{
  # Boot with the bottom panel unplugged, and plug it back in once the
  # compositor owns the display. When DSI-1 is lit at boot, before the
  # compositor (by fbcon or Plymouth), the top screen's command-mode encoder
  # later stalls in wait_for_idle on about two boots in three; with DSI-1
  # unplugged at boot there were no stalls on 3 of 3 boots, and plugging it
  # back in after the compositor started left the top unaffected
  # (README.md, "Top-screen stall: DSI-1 lit at boot is the trigger").
  boot.kernelParams = [ "video=${bottom}:d" ];

  # Writing `on` to the connector's status forces it connected, overriding
  # video=DSI-1:d. Sway sees a hot-plugged DSI-1 and applies its
  # `output DSI-1 power off`, so the bottom stays dark in this slice. The
  # unit is pulled in by every compositor start and needs the compositor to
  # be active (Requisite), so it never runs without a compositor holding
  # the display. It neither requires nor binds the compositor, so it never
  # restarts it. A second run sees the connector already connected and
  # does nothing.
  systemd.services.thor-bottom-panel-replug = {
    description = "Plug the Thor's bottom panel back in after the compositor starts";
    wantedBy = [ "korri-compositor.service" ];
    after = [ "korri-compositor.service" ];
    requisite = [ "korri-compositor.service" ];
    serviceConfig.Type = "oneshot";
    script = ''
      status=/sys/class/drm/card0-${bottom}/status
      if [ ! -e "$status" ]; then
        echo "no ${bottom} connector at $status"
        exit 0
      fi
      if [ "$(${pkgs.coreutils}/bin/cat "$status")" = connected ]; then
        echo "${bottom} already connected"
        exit 0
      fi
      echo on > "$status"
      echo "${bottom} plugged in: $(${pkgs.coreutils}/bin/cat "$status")"
    '';
  };

  services.korriLinuxInput.provider.extraDataPackages = [ thorInputplumberData ];

  # USB gadget identity. 10.42.7.0/24 is taken by the Galaxy Tab S6 Korri
  # gadget (serial tabs6-nixos), which shares the dock; on 2026-11-18 both
  # gadgets gave zao two 10.42.7.0/24 routes. 10.42.0-7 are taken, so the
  # Thor uses 10.42.8.1. The MACs are "THR" in ASCII after the shared 02:52
  # prefix and do not derive from the subnet.
  services.korriProduct.usbGadget = {
    name = "thor";
    product = "Thor NixOS";
    address = "10.42.8.1";
    hostMac = "02:52:54:48:52:01";
    deviceMac = "02:52:54:48:52:02";
  };

  services.korriLinuxHost = {
    label = "thor";
    compositor = {
      backend = "drm";
      drmDevice = "/dev/dri/card0";
      renderDevice = "/dev/dri/renderD128";
      mainDisplay = top;
      # Both panels as Korri displays (ADR 0039, ADR 0086), as the RG DS
      # declares its two.
      #
      # Modes: the top is the first (120 Hz) mode of icna3520_modes in kernel
      # patch 0057_Chipone-ICNA35XX-panel.patch. The bottom's only mode is
      # rpminiv2_display_mode in 0057_DDIC-CH13726A-panel.patch, which Sway
      # reported as 1080x1240@59.999 on 2026-11-20. Korri's mode string
      # takes whole hertz, and Sway's set_mode picks the same-size mode with
      # the closest refresh, so 60Hz selects it.
      #
      # Transform: both panels are portrait and shown landscape with 90
      # (nix-on-rocks guest/modules/chipsets/sm8550/default.nix, validated
      # under ROCKNIX 2026-05-08; the shell on DSI-2 at 90 on this stack). In
      # Sway's layout the top is 1920x1080 and the bottom 1240x1080 (Sway
      # outputs after a re-plug, 2026-11-20).
      #
      # Position: the bottom sits under the top, centred: x = (1920 - 1240)
      # / 2 = 340, y = 1080.
      #
      # Touch: devicetree paths of the two ft5x06 controllers (pinned DTS:
      # top_touchscreen at i2c4 = i2c@a90000 under geniqup@ac0000, bottom at
      # i2c_hub_3 = i2c@98c000 under geniqup@9c0000, both at 0x38). Measured
      # 2026-11-20: event5 top_touchscreen at …/ac0000.geniqup/a90000.i2c/
      # i2c-4/4-0038/…, event4 bottom_touchscreen at …/9c0000.geniqup/
      # 98c000.i2c/i2c-1/1-0038/…. The DTS's touchscreen-swapped-x-y and
      # inverted-x already give landscape coordinates (bottom 0..1239 x
      # 0..1079, top 0..1919 x 0..1079), so no calibration matrix: the host
      # module's WL_OUTPUT rules map each controller to its own output.
      displays = [
        {
          name = top;
          mode = "1080x1920@120Hz";
          x = 0;
          y = 0;
          transform = "90";
          touch = "/soc@0/geniqup@ac0000/i2c@a90000/touchscreen@38";
        }
        {
          name = bottom;
          mode = "1080x1240@60Hz";
          x = 340;
          y = 1080;
          transform = "90";
          touch = "/soc@0/geniqup@9c0000/i2c@98c000/touchscreen@38";
        }
      ];
      renderer = "gles2";
      localInput.enable = true;
      # The generated configuration powers DSI-1 off at start, as for every
      # display that is not the main one. Never power off DSI-2 by hand.
    };
  };
}

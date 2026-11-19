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

  # The kernel names each ft5x06 controller after its DT `label` (kernel
  # patch 0015-touchscreen-edt-ft5x06-allow-to-override-input-name.patch;
  # qcs8550-ayn-thor.dts: top_touchscreen on i2c4, bottom_touchscreen on
  # i2c_hub_3). The bottom panel stays dark, so libinput must not see its
  # touch controller at all; otherwise `type:touch` would map bottom taps onto
  # the top screen (legacy d152fb370 disabled it with the panel).
  services.udev.extraRules = ''
    SUBSYSTEM=="input", KERNEL=="event*", ATTRS{name}=="bottom_touchscreen", ENV{LIBINPUT_IGNORE_DEVICE}="1"
  '';

  services.korriLinuxHost = {
    label = "thor";
    compositor = {
      backend = "drm";
      drmDevice = "/dev/dri/card0";
      renderDevice = "/dev/dri/renderD128";
      mainDisplay = top;
      displays = [
        {
          name = top;
          mode = "1080x1920@120Hz";
          x = 0;
          y = 0;
          touch = null;
        }
      ];
      renderer = "gles2";
      localInput.enable = true;
      # The bottom panel is not a Korri display in this slice. Sway lights
      # every connected output by default, so turn it off once at start, as
      # legacy did (powerOnBoot = false). Never do this to DSI-2.
      extraConfig = ''
        output ${top} transform 90
        output ${bottom} power off
        input type:touch map_to_output ${top}
      '';
    };
  };
}

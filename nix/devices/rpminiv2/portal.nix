# RP Mini V2 hardware facts and recorded hardware limits for the shared Korri
# product. The console configuration in default.nix remains the recovery image.
{
  config,
  korri,
  lib,
  pkgs,
  rpminiKorriKernel,
  ...
}:
let
  system = pkgs.stdenv.hostPlatform.system;
  thermalSnapshot = pkgs.writeShellScript "rpminiv2-thermal-readonly" (
    builtins.readFile ./thermal-readonly.sh
  );
  # ROCKNIX's UCM for the RetroidPocket card; stock alsa-ucm-conf has none.
  ucm = pkgs.callPackage ./ucm { };
  ucmDirectory = "${ucm}/share/alsa/ucm2";
in
{
  # USB gadget identity. The device tree enables one controller, usb@a600000
  # (dwc3, dr_mode otg with a role switch), so exactly one UDC is expected.
  services.korriProduct.usbGadget = {
    name = "rpminiv2";
    product = "RP Mini V2 NixOS";
    # The next free /24 after the RG353M (10.42.0), RG DS (10.42.1) and
    # R36T Max (10.42.2). "PM2" in ASCII after the shared 02:52 prefix.
    address = "10.42.3.1";
    hostMac = "02:52:50:4d:32:01";
    deviceMac = "02:52:50:4d:32:02";
  };

  # The product enables CPU scaling. GPU devfreq remains at the kernel default.

  # Use the board's packaged regulatory database; select an SSID after boot
  # with nmcli rather than baking credentials into an installation image.
  hardware.wirelessRegulatoryDatabase = true;
  hardware.bluetooth.enable = true;

  # No suspend or light-sleep path has been verified on this board.
  services.korriProduct.sleep.states = [ ];

  image.baseName = lib.mkForce "nixos-rpminiv2-korri";

  boot = {
    kernelPackages = lib.mkForce (pkgs.linuxPackagesFor rpminiKorriKernel);
    # g_serial would claim the controller before the composed NCM + ACM gadget.
    kernelModules = lib.mkForce [ "libcomposite" "usb_f_ncm" "usb_f_acm" "retroid" ];
  };

  # The recovery module forces graphics off. This product-only override is
  # deliberately stronger while leaving the recovery configuration untouched.
  hardware.graphics.enable = lib.mkOverride 40 true;

  # This package contains the controller profile observed for this board. The
  # shared product owns InputPlumber and inputd themselves.
  services.korriLinuxInput.provider.extraDataPackages = [
    korri.packages.${system}.rpminiv2-inputplumber-data
  ];

  # The product owns the volume, controller-activity and display-idle actions.
  # This board's InputPlumber data routes its two volume key devices to them.

  services.korriLinuxHost = {
    label = "rpminiv2";
    compositor = {
      backend = "drm";
      # The verified TTY image exposes one MSM DRM card. Stable platform links
      # and render-node timing remain part of the physical product acceptance.
      drmDevice = "/dev/dri/card0";
      renderDevice = "/dev/dri/renderD128";
      outputName = "DSI-1";
      mode = "1080x1240@60Hz";
      renderer = "gles2";
      localInput.enable = true;
      # Hardware-verified rotation for this portrait-native 1080x1240 panel
      # produces the handheld's 1240x1080 upright landscape layout.
      extraConfig = ''
        output DSI-1 transform 90 scale 1
        input "0:0:generic_ft5x06_(8d)" map_to_output DSI-1
      '';
    };
  };

  # Chromium draws on the Adreno through Mesa (accepted 2026-10-06: the GPU
  # process loads libgallium, holds renderD128 and stays up across restarts).
  # No --disable-gpu here; the product default passes no extra arguments.

  # Capture one bounded, read-only snapshot before the product session. This
  # service does not stop a hot boot; the owner chose no automatic power-off.
  # USB serial can read it with journalctl -b -u rpminiv2-thermal-snapshot.
  systemd.services.rpminiv2-thermal-snapshot = {
    description = "RP Mini V2 read-only thermal snapshot";
    wantedBy = [ "multi-user.target" ];
    before = [
      "korri-compositor.service"
      "korri-chromium-kiosk.service"
    ];
    path = [
      pkgs.coreutils
      pkgs.procps
    ];
    serviceConfig = {
      Type = "oneshot";
      ExecStart = thermalSnapshot;
      TimeoutStartSec = "10s";
      NoNewPrivileges = true;
      ProtectSystem = "strict";
      ProtectKernelTunables = true;
    };
  };

  # PipeWire opens the card through ACP. WirePlumber runs the ALSA monitor, so
  # both need the RetroidPocket UCM to find the speaker and headphone routes.
  systemd.user.services.pipewire.environment.ALSA_CONFIG_UCM2 = ucmDirectory;
  systemd.user.services.wireplumber.environment.ALSA_CONFIG_UCM2 = ucmDirectory;

  # The VA macro sometimes finishes probing before the other sound devices.
  # Retry its module once, after boot, only if the RetroidPocket card is absent.
  # Never reload an already working card or bypass the normal module checks.
  systemd.timers.rpminiv2-va-macro-retry = {
    wantedBy = [ "timers.target" ];
    timerConfig.OnBootSec = "20s";
  };
  systemd.services.rpminiv2-va-macro-retry = {
    description = "Retry RP Mini V2 VA macro if the sound card is missing";
    unitConfig.ConditionPathExists = "!/proc/asound/RetroidPocket";
    serviceConfig = {
      Type = "oneshot";
      ExecStart = pkgs.writeShellScript "rpminiv2-va-macro-retry" ''
        set -eu
        [ ! -e /proc/asound/RetroidPocket ] || exit 0
        if [ -d /sys/module/snd_soc_lpass_va_macro ]; then
          ${pkgs.kmod}/bin/modprobe -r snd-soc-lpass-va-macro
        fi
        ${pkgs.kmod}/bin/modprobe snd-soc-lpass-va-macro
      '';
      TimeoutStartSec = "15s";
    };
  };

  # PipeWire never runs the UCM boot sequences. They set ROCKNIX's speaker and
  # headphone amplifier volumes, so run them once when the card appears.
  #
  # The product kernel sees internal UFS so it can boot an internal root. Only
  # LUN 0 (Android data and any Korri partitions) is ever written. LUNs 1 to 5
  # hold the Qualcomm boot chain, the Retroid loader and modem data, so every
  # block device on them becomes read-only when it appears. This stops an
  # accidental write. It does not stop root from clearing the flag or sending
  # SCSI commands directly.
  services.udev.extraRules = ''
    SUBSYSTEM=="sound", KERNEL=="controlC*", ATTRS{id}=="RetroidPocket", TAG+="systemd", ENV{SYSTEMD_WANTS}+="rpminiv2-audio-boot.service"
    ACTION=="add|change", SUBSYSTEM=="block", ENV{ID_PATH}=="platform-1d84000.ufshc-scsi-0:0:0:*", ENV{ID_PATH}!="platform-1d84000.ufshc-scsi-0:0:0:0", RUN+="${pkgs.util-linux}/bin/blockdev --setro $devnode"
  '';
  systemd.services.rpminiv2-audio-boot = {
    description = "RP Mini V2 ALSA UCM boot volumes";
    environment.ALSA_CONFIG_UCM2 = ucmDirectory;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = [
        "${pkgs.alsa-utils}/bin/alsaucm -c hw:RetroidPocket set _fboot ''"
        "${pkgs.alsa-utils}/bin/alsaucm -c hw:RetroidPocket set _boot ''"
      ];
      TimeoutStartSec = "20s";
    };
  };

  assertions = [
    {
      assertion = config.boot.kernelPackages.kernel.kernelConfig == ./kernel/config-korri;
      message = "The RP Mini V2 product must use the checked ROCKNIX baseline plus the Korri delta.";
    }
    {
      assertion =
        lib.elem "usb_f_acm" config.boot.kernelModules
        && !(lib.elem "g_serial" config.boot.kernelModules)
        && config.systemd.services ? usb-gadget;
      message = "The RP Mini V2 product must carry its USB serial console in the composed gadget.";
    }
  ];
}

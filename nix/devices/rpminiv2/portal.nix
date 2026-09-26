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
  host = config.services.korriLinuxHost;
  thermalSnapshot = pkgs.writeShellScript "rpminiv2-thermal-readonly" (
    builtins.readFile ./thermal-readonly.sh
  );
  displayIdle = pkgs.writeShellScript "rpminiv2-display-idle" ''
    exec ${pkgs.swayidle}/bin/swayidle -w \
      timeout 300 '${pkgs.sway}/bin/swaymsg output DSI-1 power off' \
      resume '${pkgs.sway}/bin/swaymsg output DSI-1 power on' \
      before-sleep '${pkgs.sway}/bin/swaymsg output DSI-1 power off'
  '';
  # ROCKNIX's UCM for the RetroidPocket card; stock alsa-ucm-conf has none.
  ucm = pkgs.callPackage ./ucm { };
  ucmDirectory = "${ucm}/share/alsa/ucm2";
in
{
  imports = [
    ./game-plugins.nix
    ../../base/clock-governor.nix
  ];

  # Only CPU scaling is selected here. GPU devfreq remains at the kernel default.
  services.korri.clockGovernor.enable = true;

  # Use the board's packaged regulatory database; select an SSID after boot
  # with nmcli rather than baking credentials into an installation image.
  hardware.wirelessRegulatoryDatabase = true;
  hardware.bluetooth.enable = true;

  # No suspend or light-sleep path has been verified on this board.
  services.korriProduct.sleep.states = [ ];

  image.baseName = lib.mkForce "nixos-rpminiv2-korri";

  boot = {
    kernelPackages = lib.mkForce (pkgs.linuxPackagesFor rpminiKorriKernel);
    # The bounded kernel has atkbd, ctr, loop and uinput built in and no TUN or
    # kTLS. Keep only the two real root-time modules.
    kernelModules = lib.mkForce [
      "g_serial"
      "retroid"
    ];
  };

  # The recovery module forces graphics off. This product-only override is
  # deliberately stronger while leaving the recovery configuration untouched.
  hardware.graphics.enable = lib.mkOverride 40 true;

  # This package contains the controller profile observed for this board. The
  # shared product owns InputPlumber and inputd themselves.
  services.korriLinuxInput.provider.extraDataPackages = [
    korri.packages.${system}.rpminiv2-inputplumber-data
  ];

  # InputPlumber routes the two dedicated key devices to inputd's direct
  # actions. Run wpctl as the runtime user against that user's PipeWire graph;
  # no compositor key bindings are needed.
  services.korriLinuxInput.inputd.actions = {
    volume-up = {
      command = [ "${pkgs.wireplumber}/bin/wpctl" "set-volume" "@DEFAULT_AUDIO_SINK@" "5%+" ];
      environment.XDG_RUNTIME_DIR = "/run/user/${toString host.runtimeUid}";
    };
    volume-down = {
      command = [ "${pkgs.wireplumber}/bin/wpctl" "set-volume" "@DEFAULT_AUDIO_SINK@" "5%-" ];
      environment.XDG_RUNTIME_DIR = "/run/user/${toString host.runtimeUid}";
    };
  };

  # The launcher takes InputPlumber data from the active bundle, not the
  # service's XDG_DATA_DIRS. Use the same resolved device data in both places.
  services.korriBundle.initialPackage = import ../../../services/inputd/nix/korri-bundle.nix {
    inherit pkgs;
    inputdPackage = korri.packages.${system}.korri-inputd;
    inputplumberKorri = config.services.inputplumber.package;
    korridPackage = korri.packages.${system}.korrid;
  };

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
      '';
    };
  };

  # Recorded limit: Chromium's GPU path has not been accepted on this device.
  services.korri.compositor.kiosk.extraChromiumArgs = [ "--disable-gpu" ];

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

  # Protect the OLED after five graphical idle minutes. The TTY keeps its
  # separate consoleblank=60 policy. Local power and volume input can wake Sway.
  systemd.services.rpminiv2-display-idle = {
    description = "RP Mini V2 OLED idle protection";
    wantedBy = [ "multi-user.target" ];
    requires = [ "korri-compositor.service" ];
    after = [ "korri-compositor.service" ];
    environment = {
      XDG_RUNTIME_DIR = "/run/user/${toString host.runtimeUid}";
      WAYLAND_DISPLAY = "korri-wayland";
      SWAYSOCK = "/run/korri-compositor/sway-ipc.sock";
    };
    serviceConfig = {
      User = host.runtimeUser;
      Group = host.runtimeGroup;
      ExecStart = displayIdle;
      Restart = "always";
      RestartSec = 1;
      NoNewPrivileges = true;
      PrivateTmp = true;
      ProtectSystem = "strict";
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
  services.udev.extraRules = ''
    SUBSYSTEM=="sound", KERNEL=="controlC*", ATTRS{id}=="RetroidPocket", TAG+="systemd", ENV{SYSTEMD_WANTS}+="rpminiv2-audio-boot.service"
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
      message = "The RP Mini V2 product must use the Korri kernel profile.";
    }
    {
      assertion = lib.elem "g_serial" config.boot.kernelModules;
      message = "The RP Mini V2 product must retain USB serial recovery.";
    }
  ];
}

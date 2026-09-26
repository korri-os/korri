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
  # One configfs gadget carries the serial console (ttyGS0) and an NCM link.
  # If it cannot bind, g_serial restores the console alone.
  usbAddress = "10.55.0.2/24";
  usbGadget = pkgs.writeShellScript "rpminiv2-usb-gadget" ''
    set -eu
    PATH=${lib.makeBinPath [ pkgs.coreutils pkgs.iproute2 pkgs.kmod ]}
    g=/sys/kernel/config/usb_gadget/korri
    if [ ! -d "$g" ]; then
      mkdir -p "$g"
      cd "$g"
      echo 0x1d6b > idVendor
      echo 0x0104 > idProduct
      mkdir -p strings/0x409 configs/c.1/strings/0x409
      echo Korri > strings/0x409/manufacturer
      echo "Retroid Pocket Mini V2" > strings/0x409/product
      echo "serial + ncm" > configs/c.1/strings/0x409/configuration
      mkdir functions/acm.GS0 functions/ncm.usb0
      echo 02:4b:52:00:00:02 > functions/ncm.usb0/dev_addr
      echo 02:4b:52:00:00:01 > functions/ncm.usb0/host_addr
      ln -s "$g/functions/acm.GS0" configs/c.1/
      ln -s "$g/functions/ncm.usb0" configs/c.1/
      ls /sys/class/udc | head -n 1 > UDC
    fi
    ifname="$(cat "$g/functions/ncm.usb0/ifname")"
    ip link set "$ifname" up
    ip address replace ${usbAddress} dev "$ifname"
  '';
  usbGadgetFallback = pkgs.writeShellScript "rpminiv2-usb-gadget-fallback" ''
    [ "$SERVICE_RESULT" = success ] || ${pkgs.kmod}/bin/modprobe g_serial
  '';
  # ROCKNIX's UCM for the RetroidPocket card; stock alsa-ucm-conf has none.
  ucm = pkgs.callPackage ./ucm { };
  ucmDirectory = "${ucm}/share/alsa/ucm2";
in
{
  imports = [ ./game-plugins.nix ];

  # No suspend or light-sleep path has been verified on this board.
  services.korriProduct.sleep.states = [ ];

  image.baseName = lib.mkForce "nixos-rpminiv2-korri";

  boot = {
    kernelPackages = lib.mkForce (pkgs.linuxPackagesFor rpminiKorriKernel);
    # The bounded kernel has atkbd, ctr, loop and uinput built in and no TUN or
    # kTLS. Keep only the two real root-time modules.
    # The composite gadget replaces g_serial, which stays as the fallback.
    kernelModules = lib.mkForce [ "retroid" ];
  };

  # The recovery module forces graphics off. This product-only override is
  # deliberately stronger while leaving the recovery configuration untouched.
  hardware.graphics.enable = lib.mkOverride 40 true;

  # This package contains the controller profile observed for this board. The
  # shared product owns InputPlumber and inputd themselves.
  services.korriLinuxInput.provider.extraDataPackages = [
    korri.packages.${system}.rpminiv2-inputplumber-data
  ];

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
        input type:touch map_to_output DSI-1
        bindsym --locked XF86AudioRaiseVolume exec ${pkgs.wireplumber}/bin/wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+
        bindsym --locked XF86AudioLowerVolume exec ${pkgs.wireplumber}/bin/wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%-
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

  systemd.services.rpminiv2-usb-gadget = {
    description = "RP Mini V2 USB serial and network gadget";
    wantedBy = [ "multi-user.target" ];
    after = [ "sys-kernel-config.mount" "systemd-udevd.service" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = usbGadget;
      ExecStopPost = usbGadgetFallback;
    };
  };
  networking.networkmanager.unmanaged = [ "interface-name:usb0" ];

  # PipeWire opens the card through ACP. WirePlumber runs the ALSA monitor, so
  # both need the RetroidPocket UCM to find the speaker and headphone routes.
  systemd.user.services.pipewire.environment.ALSA_CONFIG_UCM2 = ucmDirectory;
  systemd.user.services.wireplumber.environment.ALSA_CONFIG_UCM2 = ucmDirectory;

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
      assertion = config.systemd.services.rpminiv2-usb-gadget.serviceConfig.ExecStopPost == usbGadgetFallback;
      message = "The RP Mini V2 product must retain USB serial recovery.";
    }
  ];
}

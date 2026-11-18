# Device glue ported from the reference port as NixOS units: firmware search
# path, charge control, Bluetooth address, idle suspend, audio (UCM and
# WirePlumber for the four CS35L41 speakers), power key and zram.
#
# PS_HOLD power-off and reboot are kernel-side (POWER.md); nothing here.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  # Device firmware lives in /lib/firmware on the root filesystem, written by
  # the rootfs image and by deploy-over-ssh from ../firmware. It is
  # proprietary, so it is never part of the system closure: the closure can be
  # built and cached anywhere. The kernel searches /lib/firmware after
  # firmware_class.path, which NixOS points at an empty hardware.firmware set.
  charge = pkgs.writeShellApplication {
    name = "tabs6-charge";
    runtimeInputs = [
      pkgs.i2c-tools
      pkgs.util-linux
      pkgs.coreutils
    ];
    text = builtins.readFile ./device/tabs6-charge.sh;
  };

  btAddr = pkgs.writeShellApplication {
    name = "tabs6-bt-addr";
    runtimeInputs = [
      pkgs.e2fsprogs
      pkgs.bluez
      pkgs.coreutils
      pkgs.gnugrep
    ];
    text = builtins.readFile ./device/tabs6-bt-addr.sh;
  };

  idled = pkgs.writers.writePython3Bin "tabs6-idled" {
    flakeIgnore = [ "E501" ];
  } (builtins.readFile ./device/tabs6-idled.py);

  # alsa-ucm-conf plus the Tab S6 profile. The card file is named after the
  # card longname and conf.d is keyed by the driver name, sm8250 (AUDIO.md 5a).
  ucm2 = pkgs.runCommand "tabs6-ucm2" { } ''
    mkdir -p $out
    cp -r ${pkgs.alsa-ucm-conf}/share/alsa/ucm2/. $out/
    chmod -R u+w $out
    cp -r ${./device/ucm2}/. $out/
    mkdir -p $out/conf.d/sm8250
    ln -s ../../Qualcomm/sm8150/Samsung-TabS6WIFI-gts6lwifi-MTP.conf \
      $out/conf.d/sm8250/Samsung-TabS6WIFI-gts6lwifi-MTP.conf
  '';
in
lib.mkMerge [
  {
    hardware.firmware = lib.mkForce [ ];
    hardware.firmwareCompression = "none";
    hardware.enableRedistributableFirmware = false;

    # Charge current: the SM5705 powers up at USB safe defaults and nothing
    # drives the MUIC, so keep raising the limits (BATTERY.md).
    boot.kernelModules = [
      "i2c-dev"
      # One msm DRM device: KMS on the panel and the Adreno 640 render node.
      # Loaded at boot rather than on udev's schedule so the takeover from
      # simpledrm happens before the console session (or the Korri
      # compositor) starts.
      "msm"
    ];
    systemd.services.tabs6-charge = {
      description = "Tab S6 charge current control (SM5705)";
      wantedBy = [ "multi-user.target" ];
      environment = {
        TABS6_INPUT_MA = "2000";
        TABS6_CHARGE_MA = "2000";
      };
      serviceConfig = {
        ExecStart = lib.getExe charge;
        Restart = "always";
        RestartSec = 5;
      };
    };

    # Bluetooth: the WCN3990 has no public address; Samsung keeps it in
    # /efs/bluetooth/bt_addr. Read it at boot without mounting efs.
    hardware.bluetooth = {
      enable = true;
      powerOnBoot = true;
    };
    systemd.services.tabs6-bt-addr = {
      description = "Set the Tab S6 Bluetooth public address from efs";
      wantedBy = [ "bluetooth.target" ];
      after = [ "bluetooth.service" ];
      wants = [ "bluetooth.service" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = lib.getExe btAddr;
      };
    };

    # Audio: PipeWire with the UCM profile for the Secondary TDM speakers.
    security.rtkit.enable = true;
    services.pipewire = {
      enable = true;
      alsa.enable = true;
      pulse.enable = true;
      wireplumber.enable = true;
      # The speaker path is fixed at S16_LE; PipeWire's default S24_LE plays
      # silence (wireplumber/51-tabs6-audio.conf in the port).
      # The four CS35L41 amps power up with each stream and pop as they do.
      # Never suspending the sink keeps them up once WirePlumber opens it, so the
      # pop happens once per session instead of at every playback.
      wireplumber.extraConfig."51-tabs6-audio" = {
        "monitor.alsa.rules" = [
          {
            matches = [ { "node.name" = "~alsa_output.platform-sound.*"; } ];
            actions.update-props = {
              "audio.format" = "S16LE";
              "session.suspend-timeout-seconds" = 0;
            };
          }
        ];
      };
      # The microphones as a PipeWire source, outside UCM: a UCM capture device
      # takes the speaker sink down (AUDIO.md 6b).
      extraConfig.pipewire."51-tabs6-mic" = {
        "context.objects" = [
          {
            factory = "adapter";
            args = {
              "factory.name" = "api.alsa.pcm.source";
              "node.name" = "alsa_input.tabs6.mic";
              "node.description" = "Tab S6 Microphones";
              "media.class" = "Audio/Source";
              "api.alsa.path" = "hw:0,1";
              "audio.format" = "S16LE";
              "audio.rate" = 48000;
              "audio.channels" = 2;
              "audio.position" = [
                "FL"
                "FR"
              ];
              "node.pause-on-idle" = true;
              "priority.session" = 1500;
              "priority.driver" = 1500;
            };
          }
        ];
      };
    };
    # PipeWire and WirePlumber read the Tab S6 UCM profile.
    environment.sessionVariables.ALSA_CONFIG_UCM2 = "${ucm2}";
    systemd.user.services.wireplumber.environment.ALSA_CONFIG_UCM2 = "${ucm2}";
    systemd.user.services.pipewire.environment.ALSA_CONFIG_UCM2 = "${ucm2}";

    hardware.graphics.enable = true;

    # zram swap, zstd. The kernel needs CONFIG_ZRAM_BACKEND_ZSTD or zstd quietly
    # falls back to lzo-rle (PORT.md gotchas).
    zramSwap = {
      enable = true;
      algorithm = "zstd";
      memoryPercent = 50;
    };

    environment.systemPackages = [
      pkgs.alsa-utils
      pkgs.i2c-tools
      pkgs.usbutils
      pkgs.pciutils
      pkgs.iw
      pkgs.htop
      pkgs.git
      # deploy-over-ssh syncs /lib/firmware with rsync.
      pkgs.rsync
    ];
  }
  # Console only (tabs6.console.enable). The Korri product declares no sleep
  # state yet, so it neither suspends on idle nor ignores the power key, and its
  # runtime user owns PipeWire.
  (lib.mkIf config.tabs6.console.enable {
    # Idle suspend from evdev activity (SLEEP.md). s2idle is the only state.
    systemd.services.tabs6-idled = {
      description = "Tab S6 idle suspend";
      wantedBy = [ "multi-user.target" ];
      path = [ config.systemd.package ];
      serviceConfig = {
        ExecStart = lib.getExe idled;
        Restart = "always";
        RestartSec = 5;
      };
    };
    # The power key (PMIC PON) is an armed wakeup source (SLEEP.md).
    services.logind.settings.Login = {
      HandlePowerKey = "ignore";
      HandlePowerKeyLongPress = "poweroff";
    };

    # Start PipeWire and WirePlumber with simon's session, not on the first
    # client. Socket activation alone left no sink until something connected,
    # and the first `wpctl status` then raced WirePlumber's UCM probe. The
    # sockets stay for clients that come early.
    #
    # Only simon's manager runs them. Root gets a user manager on every SSH
    # login; its own PipeWire would open the card first and, with suspend off,
    # never let it go, and simon's session would get no sink.
    systemd.user.services.pipewire = {
      wantedBy = [ "default.target" ];
      unitConfig.ConditionUser = "simon";
    };
    systemd.user.services.wireplumber = {
      wantedBy = [ "default.target" ];
      unitConfig.ConditionUser = "simon";
    };
    systemd.user.services.pipewire-pulse.unitConfig.ConditionUser = "simon";
  })
]

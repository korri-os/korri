# The measured Kishi/pass-through policy and the manual override share one
# serialized effect path. Every boot starts device/boost-off.
#
# Korri's product USB gadget (nix/product/usb-gadget.nix) owns the gadget
# itself. It has no notion of a role switch or of VBUS, because every other
# Korri device has a fixed peripheral port, so this stays a Tab S6 module.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.tabs6.usbRole;
  typecStatus = import ./typec-status.nix { inherit pkgs; };
  usbRole = pkgs.writeShellApplication {
    name = "tabs6-usb-role";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.i2c-tools
      pkgs.util-linux
      typecStatus
      config.systemd.package
    ];
    text = ''
      export TABS6_USB_GADGET_UNIT="''${TABS6_USB_GADGET_UNIT:-${cfg.gadgetUnit}.service}"
      ${builtins.readFile ./tabs6-usb-role.sh}
    '';
  };
in
{
  options.tabs6.usbRole = {
    gadgetUnit = lib.mkOption {
      type = lib.types.str;
      description = "The unit that binds the USB gadget. Restarted when the port returns to device role.";
    };
  };

  config = {
    # Both adapters are resolved by their OF controller names, not numbering.
    boot.kernelModules = [ "i2c-dev" ];
    environment.systemPackages = [ usbRole ];

    # Phase A observer only. No role changes, register data writes, interrupt
    # acknowledgement or firmware loading. Existing charge/boot services stay.
    systemd.services.tabs6-typec-status = {
      description = "Tab S6 USB-C read-only Phase A observer";
      wantedBy = [ "multi-user.target" ];
      after = [ "systemd-modules-load.service" ];
      path = [
        pkgs.i2c-tools
        pkgs.libgpiod
        pkgs.coreutils
      ];
      serviceConfig = {
        ExecStart = lib.getExe typecStatus;
        Restart = "on-failure";
        RestartSec = 5;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        NoNewPrivileges = true;
        CapabilityBoundingSet = "";
        RestrictAddressFamilies = [ "AF_UNIX" ];
      };
    };

    systemd.services.tabs6-typec-auto = {
      description = "Tab S6 automatic Kishi USB role and VBUS interlock";
      wantedBy = [ "multi-user.target" ];
      after = [
        "tabs6-usb-role-boot.service"
        "tabs6-usb-role-boot-vbus.service"
        "${cfg.gadgetUnit}.service"
      ];
      path = [
        pkgs.coreutils
        pkgs.i2c-tools
      ];
      environment = {
        TABS6_USB_ROLE_HELPER = lib.getExe usbRole;
        TABS6_USB_GADGET_UNIT = "${cfg.gadgetUnit}.service";
        TABS6_AUTOMATIC = "1";
      };
      serviceConfig = {
        # Keep suspend from leaving boost on while sampling is stopped.
        ExecStart = "${config.systemd.package}/bin/systemd-inhibit --what=sleep --mode=block --who=tabs6-typec-auto --why=USB-VBUS-safety ${typecStatus}/bin/tabs6-typec-auto";
        ExecStopPost = "${lib.getExe usbRole} device";
        # A failure requires an explicit restart, never another enable attempt.
        Restart = "no";
        TimeoutStopSec = 5;
        ProtectSystem = "strict";
        ProtectHome = true;
        NoNewPrivileges = true;
        ReadWritePaths = [
          "/sys"
          "/run/lock"
        ];
      };
    };

    systemd.services.tabs6-usb-role-boot = {
      description = "Tab S6 USB-C role: gadget at boot";
      wantedBy = [ "sysinit.target" ];
      before = [
        "sysinit.target"
        "${cfg.gadgetUnit}.service"
      ];
      after = [ "systemd-modules-load.service" ];
      unitConfig.DefaultDependencies = false;
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "${lib.getExe usbRole} boot-role";
      };
    };
    systemd.services.tabs6-usb-role-boot-vbus = {
      description = "Tab S6 USB-C: VBUS boost off at boot";
      wantedBy = [ "sysinit.target" ];
      after = [ "systemd-modules-load.service" ];
      unitConfig.DefaultDependencies = false;
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = "${lib.getExe usbRole} boot-vbus";
      };
    };
  };
}

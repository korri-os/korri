# The Tab S6 USB-C role: the manual switch between the gadget lifeline and a
# powered USB host, on both systems. Every boot starts as the gadget with the
# boost off. No automatic policy switches the port: README.md, "USB-C:
# lifeline or gamepad", records the proposed one.
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
      config.systemd.package
    ];
    text = builtins.readFile ./tabs6-usb-role.sh;
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
    # The SM5705 sits on I2C bus 1; tabs6-charge loads i2c-dev for it too.
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

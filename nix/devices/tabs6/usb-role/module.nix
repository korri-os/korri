# The product kernel owns the data role. User space only maintains boot
# boost-off and the temporary provider restart workaround after kernel detach.
# The console retains its operator-only host/device path. See typec-phase-b.md.
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
      pkgs.gnugrep
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

    # No I2C status daemon: the s2mm005 client now owns address 0x33.
    # This process only reads the selected role and runs the detach workaround.
    systemd.services.tabs6-typec-role-watch = lib.mkIf (!config.tabs6.console.enable) {
      description = "Tab S6 kernel USB role detach input workaround";
      wantedBy = [ "multi-user.target" ];
      after = [ "systemd-modules-load.service" ];
      environment.TABS6_USB_ROLE_HELPER = lib.getExe usbRole;
      serviceConfig = {
        ExecStart = "${typecStatus}/bin/tabs6-typec-role-watch";
        Restart = "on-failure";
        RestartSec = 1;
        ProtectSystem = "strict";
        ProtectHome = true;
        NoNewPrivileges = true;
        ReadOnlyPaths = [ "/sys" ];
      };
    };

    systemd.services.tabs6-usb-role-boot = lib.mkIf config.tabs6.console.enable {
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

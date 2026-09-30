# Product USB gadget on every device: one USB-C cable carries an NCM network
# link with a DHCP server and the ACM root console (owner decision 2026-09-30).
#
# Extracted from the four device copies (RG353M, RG DS, R36T Max, RP Mini V2).
# Their let-bindings become the device facts below; nothing else was a device
# difference. The product owns the unit, the program and the network policy.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korriProduct.usbGadget;
  configure = pkgs.callPackage ./usb-gadget-package.nix { };
  mac = lib.types.strMatching "^[0-9a-f]{2}(:[0-9a-f]{2}){5}$";
in
{
  options.services.korriProduct.usbGadget = {
    name = lib.mkOption {
      type = lib.types.strMatching "^[a-z0-9]+$";
      description = "Configfs gadget name and serial-number stem. Distinct per device model so several devices can share one workstation.";
    };
    product = lib.mkOption {
      type = lib.types.str;
      description = "USB product string, for example \"RG353M NixOS\".";
    };
    address = lib.mkOption {
      type = lib.types.strMatching "^10\\.42\\.[0-9]+\\.1$";
      description = "The device's address on its own 10.42.N.0/24 cable network.";
    };
    hostMac = lib.mkOption {
      type = mac;
      description = "NCM host-side MAC address, lower case.";
    };
    deviceMac = lib.mkOption {
      type = mac;
      description = "NCM device-side MAC address, lower case.";
    };
    udc = lib.mkOption {
      type = lib.types.nullOr (lib.types.strMatching "^[A-Za-z0-9._-]+$");
      default = null;
      description = "The one USB device controller to bind. Null means exactly one controller must register.";
    };
  };

  config = {
    # libcomposite rather than the legacy g_serial: that module owns the
    # controller exclusively and cannot add a network function beside the
    # console.
    boot.kernelModules = [
      "libcomposite"
      "usb_f_ncm"
      "usb_f_acm"
    ];

    systemd.services.usb-gadget = {
      description = "Korri USB gadget: NCM ethernet and ACM serial";
      wantedBy = [ "multi-user.target" ];
      after = [ "sys-kernel-config.mount" ];
      requires = [ "sys-kernel-config.mount" ];
      before = [ "network-pre.target" ];
      wants = [ "network-pre.target" ];
      environment = {
        KORRI_USB_GADGET_NAME = cfg.name;
        KORRI_USB_GADGET_PRODUCT = cfg.product;
        KORRI_USB_GADGET_HOST_MAC = cfg.hostMac;
        KORRI_USB_GADGET_DEVICE_MAC = cfg.deviceMac;
        KORRI_USB_GADGET_UDC = if cfg.udc == null then "" else cfg.udc;
      };
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        ExecStart = lib.getExe configure;
      };
      preStop = ''
        # Configfs returns ENODEV when an already-unbound gadget is unbound.
        udc=/sys/kernel/config/usb_gadget/$KORRI_USB_GADGET_NAME/UDC
        if [ -e "$udc" ] && [ -n "$(cat "$udc")" ]; then
          echo "" > "$udc"
        fi
      '';
    };

    # Start the USB login when the gadget tty appears, using systemd's own
    # device-driven mechanism. Declaring the instance in NixOS instead produced
    # a unit that ran before /dev/ttyGS0 existed and never restarted (RG DS),
    # and an instance drop-in would discard the packaged agetty and autologin.
    services.udev.extraRules = ''
      SUBSYSTEM=="tty", KERNEL=="ttyGS0", TAG+="systemd", ENV{SYSTEMD_WANTS}+="serial-getty@ttyGS0.service"
    '';

    # The device side of the link. NetworkManager must leave it alone.
    networking.networkmanager.unmanaged = [ "usb0" ];

    # The link hands the workstation an address, so the DHCP server must be
    # able to answer on this interface. This opens the one port that serves the
    # lease, on the cable only, and nothing else.
    networking.firewall.interfaces.usb0.allowedUDPPorts = [ 67 ];
    systemd.network = {
      enable = true;
      networks."10-usb-gadget" = {
        matchConfig.Name = "usb0";
        address = [ "${cfg.address}/24" ];
        networkConfig = {
          DHCPServer = true;
          IPv6AcceptRA = false;
        };
        dhcpServerConfig = {
          PoolOffset = 10;
          PoolSize = 20;
          EmitDNS = false;
          EmitRouter = false;
        };
        linkConfig.RequiredForOnline = false;
      };
    };
  };
}

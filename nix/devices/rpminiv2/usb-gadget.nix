# Product only: one USB-C cable carries a network link and the root console.
#
# The recovery image keeps the legacy g_serial module and its hardware-proven
# console. That module owns the controller exclusively and cannot add a network
# function beside the console, so the product composes the same NCM plus ACM
# gadget as the RG353M, RG DS and R36T Max instead. The ACM function creates
# /dev/ttyGS0, and the udev rule in sd-image.nix still starts the root getty on
# it. portal.nix owns the product's root-time module list.
#
# Copied from nix/devices/rgds/usb-gadget.nix with this device's gadget name,
# addresses and a stricter controller selection.
{ lib, pkgs, ... }:

let
  # The next free /24 after the RG353M (10.42.0), RG DS (10.42.1) and
  # R36T Max (10.42.2).
  gadgetAddress = "10.42.3.1";
  gadgetPrefix = 24;
  hostMac = "02:52:50:4d:32:01";
  deviceMac = "02:52:50:4d:32:02";
  configfsGadget = "/sys/kernel/config/usb_gadget/rpminiv2";
  gadgetConfigure = pkgs.callPackage ./usb-gadget-package.nix {
    inherit deviceMac hostMac;
  };
in
{
  systemd.services.usb-gadget = {
    description = "RP Mini V2 USB gadget: NCM ethernet and ACM serial";
    wantedBy = [ "multi-user.target" ];
    after = [ "sys-kernel-config.mount" ];
    requires = [ "sys-kernel-config.mount" ];
    before = [ "network-pre.target" ];
    wants = [ "network-pre.target" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = lib.getExe gadgetConfigure;
    };
    preStop = ''
      # Configfs returns ENODEV when an already-unbound gadget is unbound.
      if [ -e ${configfsGadget}/UDC ] && [ -n "$(cat ${configfsGadget}/UDC)" ]; then
        echo "" > ${configfsGadget}/UDC
      fi
    '';
  };

  # The device side of the link. NetworkManager must leave it alone.
  networking.networkmanager.unmanaged = [ "usb0" ];

  # The link hands the workstation an address, so the DHCP server must be able
  # to answer on this interface. This opens the one port that serves the lease,
  # on the cable only, and nothing else.
  networking.firewall.interfaces.usb0.allowedUDPPorts = [ 67 ];
  systemd.network = {
    enable = true;
    networks."10-usb-gadget" = {
      matchConfig.Name = "usb0";
      address = [ "${gadgetAddress}/${toString gadgetPrefix}" ];
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
}

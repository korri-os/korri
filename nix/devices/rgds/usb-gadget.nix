# One USB-C cable carries both a network link and the recovery console.
#
# The first RG DS images used the legacy g_serial module, which offers only the
# console. Without a network path, every change meant removing the card and
# rewriting it. This composes the same NCM plus ACM gadget the RG353M uses, so
# a workstation can reach the device over the cable it is already plugged into.
#
# The board's device tree selects peripheral mode on usb_host0_xhci, so this
# port is a gadget and never a host.
{ lib, pkgs, ... }:

let
  gadgetAddress = "10.42.1.1";
  gadgetPrefix = 24;
  hostMac = "02:52:47:52:47:01";
  deviceMac = "02:52:47:52:47:02";
  configfsGadget = "/sys/kernel/config/usb_gadget/rgds";
  gadgetConfigure = pkgs.callPackage ./usb-gadget-package.nix {
    inherit deviceMac hostMac;
  };
in
{
  # libcomposite replaces g_serial: the legacy module owns the controller
  # exclusively and cannot add a network function beside the console.
  boot.kernelModules = lib.mkForce [
    "libcomposite"
    "usb_f_ncm"
    "usb_f_acm"
    "goodix_ts"
  ];

  systemd.services.usb-gadget = {
    description = "RG DS USB gadget: NCM ethernet and ACM serial";
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
      if ! echo "" > ${configfsGadget}/UDC; then
        echo "failed to unbind the RG DS USB gadget" >&2
      fi
    '';
  };

  # The device side of the link. NetworkManager must leave it alone.
  networking.networkmanager.unmanaged = [ "usb0" ];

  # The link hands the workstation an address, so the DHCP server must be able
  # to answer on this interface. Without this the cable comes up and stays
  # silent, and the workstation needs an address set by hand. This opens the
  # one port that serves the lease, on the cable only, and nothing else.
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

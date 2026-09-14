# One USB-C cable carries the recovery console and a network link.
#
# The board's UART5 console exists only on internal pads, so the first boot
# would otherwise need the case opened. A composed gadget gives a root console
# and an SSH-able link over the cable that is already plugged in, from the
# moment Linux brings USB up.
#
# It does not replace the UART. Anything that fails before USB init -- U-Boot,
# DRAM training, early kernel -- is still silent, and total silence is itself
# the signal to go find the pads.
#
# Ported from nix/devices/rgds/usb-gadget.nix, which proves the shape on a
# shipping device. Deliberately a copy rather than a shared module: this is a
# spike, the RG DS is an RK3568 with dwc3 while this is PX30 with dwc2, and the
# right time to extract is when a third device wants it.
{ lib, pkgs, ... }:

let
  gadgetAddress = "10.42.2.1";
  gadgetPrefix = 24;
  # Distinct from the RG DS pair so both devices can be attached to one
  # workstation at the same time.
  hostMac = "02:52:33:36:54:01";
  deviceMac = "02:52:33:36:54:02";
  configfsGadget = "/sys/kernel/config/usb_gadget/r36tmax";
  gadgetConfigure = pkgs.callPackage ./usb-gadget-package.nix {
    inherit deviceMac hostMac;
  };
in
{
  # libcomposite rather than the legacy g_serial: that module owns the
  # controller exclusively and cannot add a network function beside the
  # console.
  boot.kernelModules = lib.mkForce [
    "libcomposite"
    "usb_f_ncm"
    "usb_f_acm"
  ];

  systemd.services.usb-gadget = {
    description = "R36T Max USB gadget: NCM ethernet and ACM serial";
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
        echo "failed to unbind the R36T Max USB gadget" >&2
      fi
    '';
  };

  # Start the USB login when the gadget tty appears, using systemd's own
  # device-driven mechanism. Declaring the instance in NixOS instead produces a
  # unit that runs before /dev/ttyGS0 exists and never restarts, and an
  # instance drop-in would discard the packaged agetty and root autologin.
  services.udev.extraRules = ''
    SUBSYSTEM=="tty", KERNEL=="ttyGS0", TAG+="systemd", ENV{SYSTEMD_WANTS}+="serial-getty@ttyGS0.service"
  '';

  # The link hands the workstation an address, so the DHCP server must be able
  # to answer on this interface. Without it the cable comes up silent and the
  # workstation needs an address set by hand.
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

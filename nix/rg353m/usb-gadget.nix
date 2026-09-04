# USB gadget for autonomous access over the RG353M's USB-C port.
#
# The RG353P device tree sets usb@fcc00000 to dr_mode = "peripheral", the same
# controller Android used for adb. Expose an NCM ethernet link and an ACM
# serial console so a workstation can reach the device with one cable.
{ pkgs, ... }:

let
  gadgetAddress = "10.42.0.1";
  gadgetPrefix = 24;
  hostMac = "02:52:47:35:33:01";
  deviceMac = "02:52:47:35:33:02";
  configfsGadget = "/sys/kernel/config/usb_gadget/rg353m";
in
{
  boot.kernelModules = [
    "libcomposite"
    "usb_f_ncm"
    "usb_f_acm"
  ];

  systemd.services.usb-gadget = {
    description = "RG353M USB gadget: NCM ethernet and ACM serial";
    wantedBy = [ "multi-user.target" ];
    after = [ "sys-kernel-config.mount" ];
    requires = [ "sys-kernel-config.mount" ];
    before = [ "network-pre.target" ];
    wants = [ "network-pre.target" ];
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
    };
    path = [ pkgs.coreutils ];
    script = ''
      set -euo pipefail
      gadget=${configfsGadget}
      mkdir -p "$gadget"
      cd "$gadget"
      echo 0x1d6b > idVendor
      echo 0x0104 > idProduct
      echo 0x0100 > bcdDevice
      echo 0x0200 > bcdUSB
      mkdir -p strings/0x409
      echo "rg353m-nixos" > strings/0x409/serialnumber
      echo "Korri" > strings/0x409/manufacturer
      echo "RG353M NixOS" > strings/0x409/product
      mkdir -p configs/c.1/strings/0x409
      echo "NCM + ACM" > configs/c.1/strings/0x409/configuration
      echo 250 > configs/c.1/MaxPower
      mkdir -p functions/ncm.usb0
      echo ${hostMac} > functions/ncm.usb0/host_addr
      echo ${deviceMac} > functions/ncm.usb0/dev_addr
      mkdir -p functions/acm.usb0
      ln -sf functions/ncm.usb0 configs/c.1/
      ln -sf functions/acm.usb0 configs/c.1/
      udc="$(ls /sys/class/udc | head -n 1)"
      test -n "$udc"
      echo "$udc" > UDC
    '';
    preStop = ''
      echo "" > ${configfsGadget}/UDC || true
    '';
  };

  # The host side of the NCM link. NetworkManager must leave it alone.
  networking.networkmanager.unmanaged = [ "usb0" ];

  # services.openssh.openFirewall only opens the port on interfaces present at
  # build time. usb0 appears at runtime when the gadget binds, so SSH over the
  # cable is dropped without an explicit interface rule.
  networking.firewall.interfaces.usb0.allowedTCPPorts = [ 22 ];
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

  # Root console on the ACM serial port for recovery without networking.
  systemd.services."serial-getty@ttyGS0" = {
    enable = true;
    wantedBy = [ "getty.target" ];
    after = [ "usb-gadget.service" ];
    requires = [ "usb-gadget.service" ];
  };
}

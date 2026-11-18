# The lifeline: a USB gadget (RNDIS network + ACM serial) with a static
# address, a DHCP server for the host, sshd with key-only root login, and a
# root shell on the ACM port. None of it depends on NetworkManager, the
# desktop or Wi-Fi, so a broken session cannot lock us out (PORT.md lessons).
#
# Change the gadget shape by editing this module and rebooting, never by
# rebinding the UDC live: that drops the serial console you are typing in
# (USB_NETWORKING.md section 5).
# Console only (tabs6.console.enable): the Korri product has its own NCM +
# ACM gadget and SSH plugin.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  address = "172.16.42.1";
  prefix = 24;
  # Simon's id_ed25519.pub; the installer initramfs reads the same file.
  simonKey = lib.removeSuffix "\n" (builtins.readFile ./simonwjackson.pub);

  gadget = pkgs.writeShellApplication {
    name = "tabs6-usb-gadget";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.kmod
    ];
    # The tabs6 repository's installer initramfs runs its own copy with busybox sh.
    text = builtins.readFile ./usb-gadget.sh;
  };
in
lib.mkIf config.tabs6.console.enable {
  # tabs6-usb-role rebinds this gadget when the port returns to device role.
  tabs6.usbRole.gadgetUnit = "tabs6-usb-gadget";

  systemd.services.tabs6-usb-gadget = {
    description = "Tab S6 USB gadget (RNDIS network + ACM serial)";
    wantedBy = [ "sysinit.target" ];
    after = [ "sys-kernel-config.mount" ];
    requires = [ "sys-kernel-config.mount" ];
    before = [
      "systemd-networkd.service"
      "serial-getty@ttyGS0.service"
    ];
    unitConfig.DefaultDependencies = false;
    serviceConfig = {
      Type = "oneshot";
      RemainAfterExit = true;
      ExecStart = lib.getExe gadget;
    };
  };

  networking.useNetworkd = true;
  networking.useDHCP = false;
  networking.networkmanager.enable = false;
  systemd.network.enable = true;
  systemd.network.wait-online.enable = false;
  systemd.network.networks."10-usb-gadget" = {
    matchConfig.Name = "usb0";
    address = [ "${address}/${toString prefix}" ];
    networkConfig = {
      DHCPServer = true;
      ConfigureWithoutCarrier = true;
      LinkLocalAddressing = "no";
      IPv6AcceptRA = false;
    };
    # Hand the host one address and no default route or DNS, so plugging the
    # tablet in never steals the host's internet.
    dhcpServerConfig = {
      PoolOffset = 10;
      PoolSize = 20;
      EmitRouter = false;
      EmitDNS = false;
    };
    linkConfig.RequiredForOnline = "no";
  };
  networking.firewall.trustedInterfaces = [ "usb0" ];

  services.openssh = {
    enable = true;
    openFirewall = true;
    settings = {
      PermitRootLogin = "prohibit-password";
      PasswordAuthentication = false;
      KbdInteractiveAuthentication = false;
    };
  };
  users.users.root.openssh.authorizedKeys.keys = [ simonKey ];
  users.users.simon.openssh.authorizedKeys.keys = [ simonKey ];

  # Root shell on the ACM port, for when the network side is down. Physical
  # USB access already implies control of this unlocked device.
  systemd.services."serial-getty@ttyGS0" = {
    enable = true;
    wantedBy = [ "getty.target" ];
    serviceConfig.Restart = "always";
    serviceConfig.ExecStart = lib.mkForce [
      ""
      "@${pkgs.util-linux}/sbin/agetty agetty --login-program ${pkgs.shadow}/bin/login --autologin root --keep-baud 115200,57600,38400,9600 %I $TERM"
    ];
  };
}

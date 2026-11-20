# Wi-Fi: the WCN3990 firmware runs on the modem, so the modem boots first and
# is served its EFS (rmtfs) and firmware files (tqftpserv) before ath10k can
# reach the wlfw QMI service (WIFI.md section 2). iwd authenticates; networkd
# addresses wlan0. Credentials are provisioned later over SSH
# (iwctl station wlan0 connect <ssid>), never in the repo.
#
# The modem chain, the efs address, iwd's settings, resolved and the firewall
# serve both systems. Standalone iwd, networkd on wlan0 and Tailscale are the
# console's (tabs6.console.enable); the Korri product drives iwd through
# NetworkManager instead (../hardware.nix).
{
  config,
  lib,
  pkgs,
  ...
}:
let
  # nixpkgs points tqftpserv at /run/current-system/sw/share/uncompressed-firmware.
  # The device firmware lives in /lib/firmware on the root filesystem (see
  # device.nix), so keep upstream's path.
  tqftpserv = pkgs.tqftpserv.overrideAttrs { postPatch = ""; };

  wifiAddr = pkgs.writeShellApplication {
    name = "tabs6-wifi-addr";
    runtimeInputs = [
      pkgs.e2fsprogs
      pkgs.coreutils
      pkgs.systemd
    ];
    text = builtins.readFile ./device/tabs6-wifi-addr.sh;
  };

  wifiAddrApply = pkgs.writeShellApplication {
    name = "tabs6-wifi-addr-apply";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.gnused
      pkgs.iproute2
    ];
    text = builtins.readFile ./device/tabs6-wifi-addr-apply.sh;
  };

in
lib.mkMerge [
  {
    systemd.services.rmtfs = {
      description = "Qualcomm remote filesystem service for the modem";
      # The modem asks tqftpserv for files as soon as it boots.
      requires = [
        "dev-disk-by\\x2dpartlabel-modemst1.device"
        "tqftpserv.service"
      ];
      after = [
        "dev-disk-by\\x2dpartlabel-modemst1.device"
        "tqftpserv.service"
      ];
      serviceConfig = {
        ExecStartPre = [
          "-${pkgs.kmod}/bin/modprobe qcom_q6v5_pas"
          "-${pkgs.kmod}/bin/modprobe qrtr-smd"
          "-${pkgs.kmod}/bin/modprobe rmtfs_mem"
        ];
        # -P serves the EFS partitions by partlabel. -r keeps every modem write
        # in RAM: modemst1/modemst2/fsg are never written (AGENTS.md hard rules).
        # The port ran without -r; see README.md for that deviation.
        # -s: rmtfs starts the mpss remoteproc once it serves, and stops it when
        # it exits. Nothing else writes the remoteproc state: every extra
        # "start" bumps its refcount (DAILY_DRIVER.md).
        ExecStart = "${pkgs.rmtfs}/bin/rmtfs -P -r -s";
        Restart = "on-failure";
        RestartSec = 2;
      };
    };

    systemd.services.tqftpserv = {
      description = "QRTR TFTP server for modem firmware requests";
      serviceConfig = {
        ExecStartPre = [ "-${pkgs.kmod}/bin/modprobe qrtr-smd" ];
        ExecStart = lib.getExe tqftpserv;
        Restart = "on-failure";
        RestartSec = 2;
      };
    };

    systemd.services.tabs6-wifi = {
      description = "Tab S6 Wi-Fi bring-up (WCN3990 via the modem WLAN PD)";
      wantedBy = [ "multi-user.target" ];
      requires = [
        "rmtfs.service"
        "tqftpserv.service"
      ];
      after = [
        "rmtfs.service"
        "tqftpserv.service"
      ];
      before = [ "iwd.service" ];
      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        # The address must be in place before the driver creates wlan0.
        ExecStartPre = lib.getExe wifiAddr;
        ExecStart = "${pkgs.kmod}/bin/modprobe ath10k_snoc";
        # wlan0 appears seconds after modprobe; set the address while it is down.
        ExecStartPost = lib.getExe wifiAddrApply;
      };
    };

    networking.wireless.iwd = {
      settings = {
        General = {
          EnableNetworkConfiguration = false;
          # One address for every network: the efs address set above. iwd's
          # default is already "disabled"; it is pinned so a later default
          # cannot change the DHCP identity.
          AddressRandomization = "disabled";
        };
        # iwd deletes and recreates wlan0 at start by default; the new
        # interface takes the driver's random permanent address, not the one
        # udev set. Keep the kernel's interface.
        DriverQuirks.DefaultInterface = "ath10k_snoc";
        Settings.AutoConnect = true;
      };
    };
    services.resolved = {
      enable = true;
      settings.Resolve.MulticastDNS = true;
    };
    services.timesyncd.enable = true;

    # The port's 6.12 config had no nf_tables, only the legacy ip_tables modules
    # (DAILY_DRIVER.md). The legacy backend works either way.
    networking.nftables.enable = false;
    networking.firewall = {
      enable = true;
      package = pkgs.iptables-legacy;
    };
  }
  (lib.mkIf config.tabs6.console.enable {
    # mDNS queries from the LAN reach resolved's responder. The product image
    # gets UDP 5353 from korrid's module (ADR 0089 local discovery) instead.
    networking.firewall.interfaces.wlan0.allowedUDPPorts = [ 5353 ];
    networking.wireless.iwd.enable = true;
    systemd.network.networks."20-wlan" = {
      matchConfig.Name = "wlan0";
      networkConfig = {
        DHCP = "yes";
        IgnoreCarrierLoss = "3s";
        # tabs6.local on the LAN.
        MulticastDNS = true;
      };
      # networkd's default DHCP client ID is a DUID that changed between boots,
      # so the router gave a new lease each time even with the fixed efs address.
      dhcpV4Config.ClientIdentifier = "mac";
      linkConfig.RequiredForOnline = "no";
    };

    # Present, not logged in: `tailscale up` is a manual step over SSH.
    services.tailscale.enable = true;
  })
]

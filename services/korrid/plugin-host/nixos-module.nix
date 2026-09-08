{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korri.pluginHost;
in
{
  options.services.korri.pluginHost = {
    enable = lib.mkEnableOption "administrator-approved runtime plugins";
    package = lib.mkOption {
      type = lib.types.package;
      default = korri.packages.${pkgs.stdenv.hostPlatform.system}.korri-plugin-host;
      description = "Prebuilt generic plugin host. No plugin inventory belongs in this module.";
    };
  };
  config = lib.mkIf cfg.enable (
    lib.mkMerge [
      (import ../../../nix/device-cache/nixos-module.nix { inherit lib; })
      {
        environment.systemPackages = [ cfg.package ];
        boot.kernelModules = [ "tun" ];
        nix.settings.experimental-features = [ "nix-command" ];
        systemd.tmpfiles.rules = [
          "d /var/lib/korri-plugin-host 0700 root root -"
          "d /nix/var/nix/gcroots/korri-plugin-host 0700 root root -"
        ];
        systemd.services.korri-plugin-host = {
          description = "Restore administrator-approved runtime plugins";
          wantedBy = [ "multi-user.target" ];
          after = [
            "systemd-tmpfiles-setup.service"
            "network.target"
          ];
          serviceConfig = {
            Type = "oneshot";
            ExecStart = "${cfg.package}/bin/korri-plugin restore";
            RemainAfterExit = true;
            User = "root";
            UMask = "0077";
            TimeoutStartSec = 300;
            NoNewPrivileges = true;
            ProtectHome = true;
            ProtectSystem = "strict";
            PrivateTmp = true;
            ReadWritePaths = [
              "/var/lib/korri-plugin-host"
              "/nix/var/nix/gcroots/korri-plugin-host"
              "/run/systemd/system"
            ];
          };
        };
      }
    ]
  );
}

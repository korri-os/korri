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
    officialCatalogUrl = lib.mkOption {
      type = lib.types.nullOr (lib.types.strMatching "https://[^[:space:]]+");
      default = null;
      description = "Owner-curated HTTPS catalog URL. No production address is assumed; absence is shown explicitly by repository list.";
    };
    publishers = lib.mkOption {
      type = lib.types.attrsOf (
        lib.types.submodule {
          options = {
            publicKey = lib.mkOption {
              type = lib.types.str;
              description = "Full Nix Ed25519 public key bound to this publisher namespace.";
            };
            cacheUrl = lib.mkOption {
              type = lib.types.str;
              description = "Exact signed binary cache URL bound to this publisher namespace.";
            };
          };
        }
      );
      default = { };
      description = "Device-owned publisher bindings. Keys are @publisher namespaces, not plugin-controlled claims.";
    };
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
        environment.etc = {
          "korri-plugin-host/publishers.json".text = builtins.toJSON cfg.publishers;
        }
        // lib.optionalAttrs (cfg.officialCatalogUrl != null) {
          "korri-plugin-host/official-catalog-url".text = cfg.officialCatalogUrl + "\n";
        };
        nix.settings.trusted-public-keys = map (binding: binding.publicKey) (
          builtins.attrValues cfg.publishers
        );
        boot.kernelModules = [ "tun" ];
        nix.settings.experimental-features = [ "nix-command" ];
        systemd.tmpfiles.rules = [
          "d /var/lib/korri-plugin-host 0700 root root -"
          # Read-only admission snapshot, never a privileged executable or socket.
          "d /run/korri-plugin-host 0755 root root -"
          "d /nix/var/nix/gcroots/korri-plugin-host 0700 root root -"
        ];
        systemd.services.korri-plugin-host = {
          description = "Restore administrator-approved runtime plugins";
          wantedBy = [ "multi-user.target" ];
          before = [ "korrid.service" ];
          restartTriggers = [ config.environment.etc."korri-plugin-host/publishers.json".source ];
          after = [
            "systemd-tmpfiles-setup.service"
            "network.target"
            "firewall.service"
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
              "/run/korri-plugin-host"
              "/nix/var/nix/gcroots/korri-plugin-host"
              "/run/systemd/system"
            ];
          };
        };
      }
    ]
  );
}

{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korri.pluginHost;
  # korrid's own module, when this device runs korrid. Its account is the only
  # peer the admin socket serves.
  korrid = config.services.korridLinuxDevice or { enable = false; };
  hardening = {
    User = "root";
    UMask = "0077";
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
    adminSocket = lib.mkOption {
      type = lib.types.str;
      default = "/run/korri-plugin-admin/admin.sock";
      readOnly = true;
      description = "korrid's plugin administration socket. Only korrid's account may connect.";
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
      # Pinned nixpkgs services/networking/ssh/sshd.nix requires these host
      # account/session facilities even for a separate key-only daemon. Do not
      # enable its listener, keygen, root credential provisioning, or overwrite
      # any existing recovery SSH/PAM configuration.
      (lib.mkIf (!config.services.openssh.enable) {
        users.users.sshd = {
          isSystemUser = true;
          group = "sshd";
          description = "SSH privilege separation user";
        };
        users.groups.sshd = { };
      })
      {
        # Recovery SSH may use UsePAM=false and provide no policy at all.
        # Default the whole service: any upstream/custom policy wins intact.
        security.pam.services.sshd = lib.mkDefault {
          startSession = true;
          showMotd = true;
          unixAuth = false;
        };
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
          # SD root files are copied by an unprivileged image builder. Before
          # restore-all reads seeded receipts, give only the host's private
          # receipt tree its required root owner. Preserve file modes and never
          # follow links into the selected packages.
          "Z /var/lib/korri-plugin-host - root root -"
          # Read-only admission snapshot, never a privileged executable or socket.
          "d /run/korri-plugin-host 0755 root root -"
          "d /nix/var/nix/gcroots/korri-plugin-host 0700 root root -"
          "Z /nix/var/nix/gcroots/korri-plugin-host - root root -"
        ];
        systemd.services.korri-plugin-host = {
          description = "Restore administrator-approved runtime plugins";
          wantedBy = [ "multi-user.target" ];
          before = [ "korrid.service" ];
          restartTriggers = [ config.environment.etc."korri-plugin-host/publishers.json".source ];
          # Nix must not fall back to /root/.cache under ProtectSystem=strict.
          environment.XDG_CACHE_HOME = "/run/korri-plugin-host";
          # Not after network.target. Boot restore checks signatures already in
          # the local store, and network.target never meant online, so the
          # cache fallback for a missing signature gained nothing from it. On
          # the RG353M it waited for the USB controller until 24 s (2026-10-01).
          after = [
            "systemd-tmpfiles-setup.service"
            "firewall.service"
          ];
          serviceConfig = hardening // {
            Type = "oneshot";
            ExecStart = "${cfg.package}/bin/korri-plugin restore-all";
            RemainAfterExit = true;
            TimeoutStartSec = 300;
          };
        };
      }
      (lib.mkIf korrid.enable {
        # korrid stays unprivileged. It reaches the host only through this
        # socket, and the server accepts only korrid's uid (SO_PEERCRED).
        systemd.sockets.korri-plugin-admin = {
          description = "korrid plugin administration socket";
          wantedBy = [ "sockets.target" ];
          before = [ "korri-plugin-admin.service" ];
          socketConfig = {
            ListenStream = cfg.adminSocket;
            SocketUser = "root";
            SocketGroup = config.systemd.services.korrid.serviceConfig.Group;
            SocketMode = "0660";
            DirectoryMode = "0755";
            RemoveOnStop = true;
            Service = "korri-plugin-admin.service";
          };
        };
        systemd.services.korri-plugin-admin = {
          description = "Serve korrid's plugin repository, install and on/off requests";
          requires = [ "korri-plugin-admin.socket" ];
          after = [
            "korri-plugin-admin.socket"
            "korri-plugin-host.service"
          ];
          restartTriggers = [ config.environment.etc."korri-plugin-host/publishers.json".source ];
          environment = {
            XDG_CACHE_HOME = "/run/korri-plugin-host";
            KORRI_PLUGIN_ADMIN_PEER_UID = toString korrid.uid;
          };
          serviceConfig = hardening // {
            ExecStart = "${cfg.package}/bin/korri-plugin serve-admin";
            Restart = "on-failure";
            RestartSec = 1;
          };
        };
      })
    ]
  );
}

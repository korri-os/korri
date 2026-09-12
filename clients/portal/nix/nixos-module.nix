{
  korri,
  chromiumArgs ? [ ],
}:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  policy = import ./runtime-policy.nix;
  cfg = config.services.korri.webSurfaceHost;
  kiosk = config.services.korri.compositor.kiosk;
  compositor = config.systemd.services.korri-compositor;
  # The existing portal package names its isolated user and systemd state.
  user = "korri-portal";
  group = user;
  home = "/var/lib/${user}";
  runtimeDir = "/run/${user}";
  display = config.systemd.services.sunshine.environment.WAYLAND_DISPLAY;
  profile = "${home}${policy.chromiumProfileSuffix}";
  origin = "http://${cfg.host}:${toString cfg.port}";
  # The surface resolver reads ?surface= first and remembers it, so a device
  # that can only open one fixed URL still picks its surface. This path serves
  # no runtime.json, so the query parameter is the seam that exists here.
  url = "${origin}/${lib.optionalString (cfg.surfaceId != null) "?surface=${cfg.surfaceId}"}";
  address = config.systemd.services.korrid.environment.KORRID_ADDRESS or "";
  addressPort = builtins.match ".*:([0-9]+)" address;
  korridOrigin =
    if addressPort == null then
      throw "The portal requires korrid's configured socket address."
    else
      "http://127.0.0.1:${builtins.head addressPort}";
  credentialService = "korri-portal-credentials.service";
  credentialDirectory = "korri-portal-credentials";
  kioskParents = [
    "nginx.service"
    "korri-compositor.service"
    "korrid.service"
    credentialService
  ];
  # The credential ID is the existing korrid capability consumer's name.
  credential = "KORRID_RPC_CAPABILITY:/run/${credentialDirectory}/KORRID_RPC_CAPABILITY";
  selector = import ./select-package.nix { inherit pkgs url; };
  shell = korri.packages.${pkgs.stdenv.hostPlatform.system}.korri-portal-shell;
  chromium = pkgs.writeShellApplication {
    name = "korri-chromium-kiosk";
    text = ''
      exec ${lib.getExe shell} ${pkgs.chromium}/bin/chromium \
        --ozone-platform=wayland \
        --kiosk \
        --user-data-dir="$KORRI_CHROMIUM_USER_DATA_DIR" \
        --no-first-run --no-default-browser-check \
        --disable-background-networking --disable-extensions --disable-sync \
        --disable-session-crashed-bubble \
        ${lib.escapeShellArgs chromiumArgs} \
        "$@"
    '';
  };
  grantWaylandAccess = pkgs.writeShellApplication {
    name = "korri-portal-wayland-access";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.acl
    ];
    text = ''
      socket=$(readlink -f ${lib.escapeShellArg "${compositor.environment.XDG_RUNTIME_DIR}/${display}"})
      [ -S "$socket" ]
      [ "$(dirname "$socket")" = ${lib.escapeShellArg compositor.environment.XDG_RUNTIME_DIR} ]
      [ "$(stat -c %U "$socket")" = ${lib.escapeShellArg compositor.serviceConfig.User} ]
      # The private bind mount preserves mode 0700 on wlroots sockets. Grant
      # this one client access to that inode, never to the runtime directory.
      setfacl -m ${lib.escapeShellArg "u:${user}:rw-"} "$socket"
    '';
  };
  createCredential = pkgs.writeShellApplication {
    name = "korri-portal-credentials";
    text = ''
      umask 077
      ${pkgs.openssl}/bin/openssl rand -hex 32 > "$RUNTIME_DIRECTORY/KORRID_RPC_CAPABILITY"
    '';
  };
  reservedEnvironment = [
    "HOME"
    "XDG_RUNTIME_DIR"
    "WAYLAND_DISPLAY"
    "DBUS_SESSION_BUS_ADDRESS"
    "CREDENTIALS_DIRECTORY"
    "KORRID_RPC_CAPABILITY"
    "KORRID_ADDRESS"
    "KORRID_PORTAL_ORIGIN"
    "KORRI_ASSET_ROOT"
    "KORRI_WEB_SURFACE_URL"
    "KORRI_CHROMIUM_USER_DATA_DIR"
  ];
in
{
  # Static serving and profile selection preserve the extracted legacy module.
  # RPC remains in korrid. This module supplies startup credentials, not a proxy.
  options.services.korri = {
    webSurfaceHost = {
      enable = lib.mkEnableOption "the local portal";
      host = lib.mkOption {
        type = lib.types.str;
        default = policy.host;
      };
      port = lib.mkOption {
        type = lib.types.port;
        default = policy.port;
      };
      surfaceId = lib.mkOption {
        type = lib.types.nullOr (lib.types.strMatching "^[a-z0-9-]+$");
        default = null;
        description = ''
          Surface this device opens with, passed to the portal's existing
          surface preference. Null keeps the portal's own default. An
          unrecognized id is ignored by the resolver, which falls back rather
          than failing. This is a display preference, not a capability.
        '';
      };
      environment = lib.mkOption {
        type = lib.types.attrsOf lib.types.str;
        default = { };
      };
    };
    compositor.kiosk.enable = lib.mkEnableOption "the Korri Chromium kiosk";
  };

  config = lib.mkMerge [
    {
      assertions = [
        {
          assertion = !kiosk.enable || cfg.enable;
          message = "Korri kiosk requires the local portal server.";
        }
        {
          assertion = !cfg.enable || cfg.host == "127.0.0.1";
          message = "The portal must listen on loopback only.";
        }
        {
          assertion = !kiosk.enable || addressPort != null;
          message = "The portal requires korrid's configured socket address.";
        }
        {
          assertion =
            !kiosk.enable || (builtins.baseNameOf display == display && display != "." && display != "..");
          message = "The portal requires the compositor's Wayland socket filename.";
        }
        {
          assertion =
            !kiosk.enable || builtins.all (name: !(builtins.hasAttr name cfg.environment)) reservedEnvironment;
          message = "Portal environment overrides cannot replace its isolated runtime or RPC credentials.";
        }
      ];
    }
    (lib.mkIf cfg.enable {
      environment.systemPackages = [ selector ];
      systemd.services.korri-portal-initialize = {
        description = "Initialize the portal's Nix profile without replacing updates";
        wantedBy = [ "multi-user.target" ];
        before = [ "nginx.service" ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          ExecStart = "${selector}/bin/korri-portal-select initialize ${
            korri.packages.${pkgs.stdenv.hostPlatform.system}.korri-portal
          }";
        };
      };
      services.nginx = {
        enable = true;
        virtualHosts.korri-portal = {
          listen = [
            {
              addr = cfg.host;
              port = cfg.port;
            }
          ];
          root = policy.assetRoot;
          locations."/".extraConfig = ''
            try_files $uri $uri/ =404;
            add_header Cache-Control "no-store" always;
            add_header Content-Security-Policy "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self' data:; connect-src ${
              if kiosk.enable then korridOrigin else "'none'"
            }; object-src 'none'; frame-ancestors 'none'" always;
          '';
        };
      };
      systemd.services.nginx = {
        requires = [ "korri-portal-initialize.service" ];
        after = [ "korri-portal-initialize.service" ];
      };
    })
    (lib.mkIf kiosk.enable {
      # Chromium reads only the validated virtual target, never raw sources.
      services.korriLinuxInput.inputd.extraActionUsers = [ user ];
      users.groups.${group} = { };
      users.users.${user} = {
        isSystemUser = true;
        inherit group home;
      };
      systemd.services.korri-portal-credentials = {
        description = "Create the portal capability for this boot";
        serviceConfig = {
          Type = "oneshot";
          User = "root";
          ExecStart = lib.getExe createCredential;
          RemainAfterExit = true;
          RuntimeDirectory = credentialDirectory;
          RuntimeDirectoryMode = "0700";
          UMask = "0077";
          NoNewPrivileges = true;
          ProtectSystem = "strict";
          ProtectHome = true;
        };
      };
      # Both consumers restart when the producer rotates its credential. A web
      # update restarts only the kiosk, leaving the producer and korrid intact.
      systemd.services.korrid = {
        wantedBy = [ credentialService ];
        requires = [ credentialService ];
        after = [ credentialService ];
        partOf = [ credentialService ];
        environment.KORRID_PORTAL_ORIGIN = origin;
        serviceConfig.LoadCredential = [ credential ];
      };
      # tmpfiles needs each parent to have the real owner before descending.
      # Moving an existing browser profile is an explicit deployment operation.
      systemd.tmpfiles.rules = [
        "d ${home} 0700 ${user} ${group} -"
        "d ${home}/.local - ${user} ${group} -"
        "d ${home}/.local/state - ${user} ${group} -"
        "d ${home}/.local/state/korri 0700 ${user} ${group} -"
        "d ${home}/.local/state/korri/chromium 0700 ${user} ${group} -"
        "d ${profile} 0700 ${user} ${group} -"
      ];
      systemd.services.korri-chromium-kiosk = {
        description = "Korri portal kiosk";
        # A recovered parent must pull the kiosk back in after a failed boot job.
        wantedBy = [ "multi-user.target" ] ++ kioskParents;
        requires = kioskParents;
        after = kioskParents;
        partOf = kioskParents;
        environment = cfg.environment // {
          HOME = home;
          XDG_RUNTIME_DIR = runtimeDir;
          WAYLAND_DISPLAY = display;
          KORRI_ASSET_ROOT = policy.assetRoot;
          KORRI_WEB_SURFACE_URL = url;
          KORRI_CHROMIUM_USER_DATA_DIR = profile;
          KORRID_ADDRESS = address;
          KORRID_PORTAL_ORIGIN = origin;
        };
        serviceConfig = {
          Type = "notify";
          NotifyAccess = "main";
          TimeoutStartSec = 90;
          KillMode = "control-group";
          User = user;
          Group = group;
          ExecStartPre = "+${lib.getExe grantWaylandAccess}";
          ExecStart = "${chromium}/bin/korri-chromium-kiosk";
          LoadCredential = [ credential ];
          StateDirectory = user;
          StateDirectoryMode = "0700";
          RuntimeDirectory = user;
          RuntimeDirectoryMode = "0700";
          # The source may be a compositor-owned symlink. Systemd resolves it
          # when it creates the private mount. Startup grants the socket ACL.
          BindReadOnlyPaths = [
            "${compositor.environment.XDG_RUNTIME_DIR}/${display}:${runtimeDir}/${display}"
          ];
          Restart = "on-failure";
          RestartSec = 5;
          UMask = "0077";
          NoNewPrivileges = true;
          ProtectSystem = "strict";
          ProtectHome = true;
          PrivateTmp = true;
          LimitCORE = 0;
          ReadWritePaths = [
            profile
            runtimeDir
          ];
        };
      };
    })
  ];
}

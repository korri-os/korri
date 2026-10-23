{
  pkgs,
  nixpkgs,
  korri,
}:
let
  evaluate =
    extra:
    nixpkgs.lib.nixosSystem {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        korri.nixosModules.korri-input
        # Even an explicit board-edge import stays inert on unpatched PID 1.
        ./kiosk-freezer.nix
        (import ./nixos-module.nix {
          inherit korri;
        })
        {
          system.stateVersion = "25.11";
          users.groups.games = { };
          users.users.gameplay = {
            isNormalUser = true;
            group = "games";
            home = "/home/gameplay";
          };
          systemd.services.korrid = {
            environment.KORRID_ADDRESS = "0.0.0.0:39217";
            serviceConfig.ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
          };
          systemd.sockets.korrid-control.socketConfig = {
            ListenStream = "/run/korrid-control/control.sock";
            SocketUser = "root";
            SocketGroup = "korri-control";
            SocketMode = "0660";
          };
          systemd.services.korri-compositor = {
            serviceConfig = {
              User = "gameplay";
              Group = "games";
              ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
            };
            environment = {
              XDG_RUNTIME_DIR = "/run/user/1001";
              DBUS_SESSION_BUS_ADDRESS = "unix:path=/run/user/1001/bus";
              KORRI_WAYLAND_DISPLAY = "korri-wayland";
            };
          };
        }
        extra
      ];
    };
  disabled = (evaluate { }).config;
  enabled =
    (evaluate {
      services.korri.webSurfaceHost.enable = true;
      services.korri.compositor.kiosk = {
        enable = true;
      };
    }).config;
  serverOnly =
    (evaluate {
      services.korri.webSurfaceHost.enable = true;
    }).config;
  alternate =
    (evaluate {
      services.korri.webSurfaceHost.enable = true;
      services.korri.compositor.kiosk.enable = true;
      systemd.services.korrid.environment.KORRID_ADDRESS = nixpkgs.lib.mkForce "[::]:43117";
    }).config;
  invalid =
    (evaluate {
      services.korri.webSurfaceHost = {
        enable = true;
        host = "0.0.0.0";
      };
    }).config;
  invalidEnvironment =
    (evaluate {
      services.korri.webSurfaceHost = {
        enable = true;
        environment.DBUS_SESSION_BUS_ADDRESS = "unix:path=/run/user/1001/bus";
      };
      services.korri.compositor.kiosk.enable = true;
    }).config;
  service = enabled.systemd.services.korri-portal-kiosk;
  daemon = enabled.systemd.services.korrid;
  credentialService = "korri-portal-credentials.service";
  credential = "KORRID_RPC_CAPABILITY:/run/korri-portal-credentials/KORRID_RPC_CAPABILITY";
  producer = enabled.systemd.services.korri-portal-credentials;
  profile = (import ./runtime-policy.nix).assetRoot;
  unit = enabled.systemd.units."korri-portal-kiosk.service".text;
  nginx = enabled.services.nginx.virtualHosts.korri-portal;
in
assert !(disabled.systemd.services ? korri-portal-kiosk);
# korrid freezes the kiosk during a focused live game. Without a kiosk the
# feature stays inert: no unit name, no grant, no thaw hooks.
assert !(disabled.systemd.services.korrid.environment ? KORRID_PORTAL_UNIT);
assert !(serverOnly.systemd.services.korrid.environment ? KORRID_PORTAL_UNIT);
assert !(disabled.systemd.services ? korri-portal-kiosk-thaw);
assert !(serverOnly.systemd.services ? korri-portal-kiosk-thaw);
assert !(nixpkgs.lib.hasInfix "korri-portal-kiosk" disabled.security.polkit.extraConfig);
assert !(nixpkgs.lib.hasInfix "korri-portal-kiosk" serverOnly.security.polkit.extraConfig);
assert (disabled.systemd.services.korrid.serviceConfig.ExecStopPost or [ ]) == [ ];
# A shared kiosk must not acquire C3 on unpatched systemd.
assert !(daemon.environment ? KORRID_PORTAL_UNIT);
assert !(enabled.systemd.services ? korri-portal-kiosk-thaw);
assert !(nixpkgs.lib.hasInfix "korri-portal-kiosk" enabled.security.polkit.extraConfig);
assert !(disabled.systemd.services ? korri-portal-initialize);
assert !(disabled.systemd.services ? korri-portal-credentials);
assert !disabled.services.nginx.enable;
assert !(disabled.services.korriLinuxInput.inputd ? extraActionUsers);
assert !(serverOnly.services.korriLinuxInput.inputd ? extraActionUsers);
assert !(enabled.services.korriLinuxInput.inputd ? extraActionUsers);
assert enabled.users.users.korri-portal.uid == null;
assert service.serviceConfig.User == "korri-portal";
assert service.serviceConfig.Group == "korri-portal";
assert enabled.users.users.korri-portal.isSystemUser;
assert enabled.users.users.korri-portal.extraGroups == [ ];
assert service.serviceConfig.Type == "notify";
assert service.serviceConfig.NotifyAccess == "main";
assert service.serviceConfig.KillMode == "control-group";
assert service.serviceConfig.TimeoutStartSec >= 60;
assert service.serviceConfig.StateDirectory == "korri-portal";
assert service.serviceConfig.StateDirectoryMode == "0700";
assert service.serviceConfig.RuntimeDirectory == "korri-portal";
assert service.serviceConfig.RuntimeDirectoryMode == "0700";
assert service.serviceConfig ? ExecStartPre;
assert nixpkgs.lib.hasPrefix "+" service.serviceConfig.ExecStartPre;
assert
  service.serviceConfig.BindReadOnlyPaths == [
    "/run/user/1001/korri-wayland:/run/korri-portal/korri-wayland"
  ];
assert service.environment.XDG_RUNTIME_DIR == "/run/korri-portal";
assert service.environment.HOME == "/var/lib/korri-portal";
assert !(service.environment ? DBUS_SESSION_BUS_ADDRESS);
assert !(service.environment ? KORRID_RPC_CAPABILITY);
assert !(daemon.environment ? KORRID_RPC_CAPABILITY);
assert builtins.elem credential service.serviceConfig.LoadCredential;
assert builtins.elem credential daemon.serviceConfig.LoadCredential;
assert service.environment.KORRID_ADDRESS == daemon.environment.KORRID_ADDRESS;
assert service.environment.KORRID_PORTAL_ORIGIN == "http://127.0.0.1:8099";
assert daemon.environment.KORRID_PORTAL_ORIGIN == service.environment.KORRID_PORTAL_ORIGIN;
assert producer.serviceConfig.Type == "oneshot";
assert producer.serviceConfig.RemainAfterExit;
assert producer.serviceConfig.User == "root";
assert producer.serviceConfig.RuntimeDirectoryMode == "0700";
assert producer.serviceConfig.UMask == "0077";
assert builtins.elem "multi-user.target" service.wantedBy;
assert builtins.all
  (
    parent:
    builtins.elem parent service.wantedBy
    && builtins.elem parent service.requires
    && builtins.elem parent service.after
    && builtins.elem parent service.partOf
  )
  [
    "korri-compositor.service"
    "nginx.service"
    credentialService
  ];
# The kiosk starts beside korrid, not after it: the browser starts while
# korrid is, and the portal waits for korrid to answer before it mounts.
assert builtins.elem "korrid.service" service.wantedBy;
assert builtins.elem "korrid.service" service.requires;
assert builtins.elem "korrid.service" service.partOf;
assert !(builtins.elem "korrid.service" service.after);
assert builtins.elem credentialService daemon.wantedBy;
assert builtins.elem credentialService daemon.requires;
assert builtins.elem credentialService daemon.after;
assert builtins.elem credentialService daemon.partOf;
assert
  enabled.systemd.sockets.korrid-control.socketConfig
  == disabled.systemd.sockets.korrid-control.socketConfig;
assert service.environment.KORRI_WEB_SURFACE_URL == "http://127.0.0.1:8099/";
assert
  enabled.services.korriLinuxInput.inputd.actions.system-panel.command == [
    "${pkgs.sway-unwrapped}/bin/swaymsg"
    "-s"
    "/run/korri-compositor/sway-ipc.sock"
    ''[app_id="org.korri.portal"] focus''
  ];
# A device that names a surface opens it through the portal's own preference
# seam; the served origin and asset root stay identical either way.
assert
  (evaluate {
    services.korri.webSurfaceHost = {
      enable = true;
      surfaceId = "pico";
    };
    services.korri.compositor.kiosk.enable = true;
  }).config.systemd.services.korri-portal-kiosk.environment.KORRI_WEB_SURFACE_URL
  == "http://127.0.0.1:8099/?surface=pico";
assert service.environment.KORRI_ASSET_ROOT == profile;
assert service.environment.XDG_DATA_HOME == "/var/lib/korri-portal/.local/share";
assert service.environment.XDG_CACHE_HOME == "/var/lib/korri-portal/.cache";
assert service.environment.KORRI_PORTAL_TRANSPARENT == "1";
assert
  service.serviceConfig.ExecStart
  == "${korri.packages.${pkgs.stdenv.hostPlatform.system}.korri-portal-wpe}/bin/korri-portal-wpe";
assert service.serviceConfig.NoNewPrivileges;
assert service.serviceConfig.RestrictNamespaces == false;
assert service.serviceConfig.ProtectSystem == "strict";
assert service.serviceConfig.ProtectHome;
assert service.serviceConfig.PrivateTmp;
assert service.serviceConfig.UMask == "0077";
assert service.serviceConfig.LimitCORE == 0;
assert (service.serviceConfig.DevicePolicy or "auto") == "auto";
assert
  service.serviceConfig.ReadWritePaths == [
    "/var/lib/korri-portal/.local/share"
    "/var/lib/korri-portal/.cache"
    "/run/korri-portal"
  ];
assert
  service.serviceConfig.ExecStart
  == alternate.systemd.services.korri-portal-kiosk.serviceConfig.ExecStart;
assert nginx.root == profile;
assert builtins.elem "d /var/lib/korri-portal 0700 korri-portal korri-portal -"
  enabled.systemd.tmpfiles.rules;
assert builtins.elem "d /var/lib/korri-portal/.local - korri-portal korri-portal -"
  enabled.systemd.tmpfiles.rules;
assert builtins.elem "d /var/lib/korri-portal/.local/share 0700 korri-portal korri-portal -"
  enabled.systemd.tmpfiles.rules;
assert builtins.elem "d /var/lib/korri-portal/.cache 0700 korri-portal korri-portal -"
  enabled.systemd.tmpfiles.rules;
assert
  map (listener: { inherit (listener) addr port; }) nginx.listen == [
    {
      addr = "127.0.0.1";
      port = 8099;
    }
  ];
assert builtins.attrNames nginx.locations == [ "/" ];
assert nixpkgs.lib.hasInfix "connect-src http://127.0.0.1:39217 ws://127.0.0.1:39217;"
  nginx.locations."/".extraConfig;
assert nixpkgs.lib.hasInfix "connect-src http://127.0.0.1:43117 ws://127.0.0.1:43117;"
  alternate.services.nginx.virtualHosts.korri-portal.locations."/".extraConfig;
assert builtins.any (
  a: !a.assertion && a.message == "The portal must listen on loopback only."
) invalid.assertions;
assert builtins.any (
  a:
  !a.assertion
  &&
    a.message == "Portal environment overrides cannot replace its isolated runtime or RPC credentials."
) invalidEnvironment.assertions;
pkgs.runCommand "korri-portal-module-check" { } ''
  grep -F 'User=korri-portal' ${pkgs.writeText "portal-kiosk.service" unit}
  grep -F 'Type=notify' ${pkgs.writeText "portal-kiosk.service" unit}
  grep -F 'Restart=on-failure' ${pkgs.writeText "portal-kiosk.service" unit}
  grep -F 'NoNewPrivileges=true' ${pkgs.writeText "portal-kiosk.service" unit}
  grep -F 'ProtectSystem=strict' ${pkgs.writeText "portal-kiosk.service" unit}
  grep -F 'PrivateTmp=true' ${pkgs.writeText "portal-kiosk.service" unit}
  export RUNTIME_DIRECTORY="$TMPDIR/credentials"
  mkdir -m 700 "$RUNTIME_DIRECTORY"
  ${producer.serviceConfig.ExecStart} > "$TMPDIR/credential-output"
  test ! -s "$TMPDIR/credential-output"
  test "$(stat -c %a "$RUNTIME_DIRECTORY/KORRID_RPC_CAPABILITY")" = 600
  grep -Eq '^[0-9a-f]{64}$' "$RUNTIME_DIRECTORY/KORRID_RPC_CAPABILITY"
  cp "$RUNTIME_DIRECTORY/KORRID_RPC_CAPABILITY" "$TMPDIR/previous-credential"
  ${producer.serviceConfig.ExecStart} > "$TMPDIR/credential-output"
  test ! -s "$TMPDIR/credential-output"
  if cmp -s "$TMPDIR/previous-credential" "$RUNTIME_DIRECTORY/KORRID_RPC_CAPABILITY"; then exit 1; fi
  touch "$out"
''

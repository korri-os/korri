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
        (import ./nixos-module.nix {
          inherit korri;
          chromiumArgs = [
            "--force-prefers-reduced-motion"
            "--disable-gpu"
          ];
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
            };
          };
          systemd.services.sunshine.environment.WAYLAND_DISPLAY = "korri-wayland";
        }
        extra
      ];
    };
  disabled = (evaluate { }).config;
  enabled =
    (evaluate {
      services.korri.webSurfaceHost.enable = true;
      services.korri.compositor.kiosk.enable = true;
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
  service = enabled.systemd.services.korri-chromium-kiosk;
  daemon = enabled.systemd.services.korrid;
  credentialService = "korri-portal-credentials.service";
  credential = "KORRID_RPC_CAPABILITY:/run/korri-portal-credentials/KORRID_RPC_CAPABILITY";
  producer = enabled.systemd.services.korri-portal-credentials;
  profile = (import ./runtime-policy.nix).assetRoot;
  unit = enabled.systemd.units."korri-chromium-kiosk.service".text;
  nginx = enabled.services.nginx.virtualHosts.korri-portal;
in
assert !(disabled.systemd.services ? korri-chromium-kiosk);
assert !(disabled.systemd.services ? korri-portal-initialize);
assert !(disabled.systemd.services ? korri-portal-credentials);
assert !disabled.services.nginx.enable;
assert disabled.services.korriLinuxInput.inputd.extraActionUsers == [ ];
assert serverOnly.services.korriLinuxInput.inputd.extraActionUsers == [ ];
assert enabled.services.korriLinuxInput.inputd.extraActionUsers == [ "korri-portal" ];
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
    "korrid.service"
    credentialService
  ];
assert builtins.elem credentialService daemon.wantedBy;
assert builtins.elem credentialService daemon.requires;
assert builtins.elem credentialService daemon.after;
assert builtins.elem credentialService daemon.partOf;
assert
  enabled.systemd.sockets.korrid-control.socketConfig
  == disabled.systemd.sockets.korrid-control.socketConfig;
assert service.environment.KORRI_WEB_SURFACE_URL == "http://127.0.0.1:8099/";
assert service.environment.KORRI_ASSET_ROOT == profile;
assert
  service.environment.KORRI_CHROMIUM_USER_DATA_DIR
  == "/var/lib/korri-portal/.local/state/korri/chromium/profile";
assert nginx.root == profile;
assert builtins.elem "d /var/lib/korri-portal 0700 korri-portal korri-portal -"
  enabled.systemd.tmpfiles.rules;
assert builtins.elem "d /var/lib/korri-portal/.local - korri-portal korri-portal -"
  enabled.systemd.tmpfiles.rules;
assert builtins.elem "d /var/lib/korri-portal/.local/state - korri-portal korri-portal -"
  enabled.systemd.tmpfiles.rules;
assert builtins.all
  (
    suffix:
    builtins.elem "d /var/lib/korri-portal/.local/state/korri${suffix} 0700 korri-portal korri-portal -" enabled.systemd.tmpfiles.rules
  )
  [
    ""
    "/chromium"
    "/chromium/profile"
  ];
assert
  map (listener: { inherit (listener) addr port; }) nginx.listen == [
    {
      addr = "127.0.0.1";
      port = 8099;
    }
  ];
assert builtins.attrNames nginx.locations == [ "/" ];
assert nixpkgs.lib.hasInfix "connect-src http://127.0.0.1:39217;" nginx.locations."/".extraConfig;
assert nixpkgs.lib.hasInfix "connect-src http://127.0.0.1:43117;"
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
  grep -F -- 'korri-portal-shell' ${service.serviceConfig.ExecStart}
  grep -F -- '--ozone-platform=wayland' ${service.serviceConfig.ExecStart}
  grep -F -- '--kiosk' ${service.serviceConfig.ExecStart}
  grep -F -- '--force-prefers-reduced-motion' ${service.serviceConfig.ExecStart}
  grep -F -- '--disable-gpu' ${service.serviceConfig.ExecStart}
  if grep -E -- '--app|--no-sandbox|--remote-debugging|DBUS_SESSION_BUS_ADDRESS' ${service.serviceConfig.ExecStart}; then exit 1; fi
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

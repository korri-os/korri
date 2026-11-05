{
  pkgs,
  nixpkgs,
  korri,
}:
let
  lib = nixpkgs.lib;
  evaluate =
    extra:
    nixpkgs.lib.nixosSystem {
      system = pkgs.stdenv.hostPlatform.system;
      modules = [
        korri.nixosModules.korri-input
        # Even an explicit board-edge import stays inert on unpatched PID 1.
        ./freezer.nix
        (import ./nixos-module.nix { inherit korri; })
        {
          system.stateVersion = "25.11";
          users.groups.games = { };
          users.users.gameplay = {
            isNormalUser = true;
            group = "games";
            home = "/home/gameplay";
          };
          systemd.services.korrid = {
            environment.KORRID_ADDRESS = "[::]:39217";
            serviceConfig.ExecStart = "${pkgs.coreutils}/bin/sleep infinity";
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
  enabled = (evaluate { services.korri.shell.enable = true; }).config;
  started =
    (evaluate {
      services.korri.shell = {
        enable = true;
        startSurface = "zone";
      };
    }).config;
  service = enabled.systemd.services.korri-shell;
  daemon = enabled.systemd.services.korrid;
  credentialService = "korri-shell-credentials.service";
  credential = "KORRID_RPC_CAPABILITY:/run/korri-shell-credentials/KORRID_RPC_CAPABILITY";
  producer = enabled.systemd.services.korri-shell-credentials;
  unit = enabled.systemd.units."korri-shell.service".text;
  installed = korri.packages.${pkgs.stdenv.hostPlatform.system}.korri-shell-surfaces;
  argv = lib.splitString " " service.serviceConfig.ExecStart;
in
assert !(disabled.systemd.services ? korri-shell);
assert !(disabled.systemd.services ? korri-shell-credentials);
# korrid freezes the shell during a focused live game. Without a shell the
# feature stays inert: no unit name, no grant, no thaw hooks.
assert !(disabled.systemd.services.korrid.environment ? KORRID_SHELL_UNIT);
assert !(lib.hasInfix "korri-shell" disabled.security.polkit.extraConfig);
assert (disabled.systemd.services.korrid.serviceConfig.ExecStopPost or [ ]) == [ ];
# The shell must not acquire freezer authority on unpatched systemd.
assert daemon.environment.KORRID_SHELL_UNIT == "korri-shell.service";
assert daemon.environment.KORRID_SHELL_FREEZER_ENABLED == "false";
assert (daemon.serviceConfig.ExecStopPost or [ ]) == [ ];
assert !(lib.hasInfix "korri-shell" enabled.security.polkit.extraConfig);
# The isolated account: a system user with no groups, never root.
assert enabled.users.users.korri-portal.isSystemUser;
assert enabled.users.users.korri-portal.uid == null;
assert enabled.users.users.korri-portal.extraGroups == [ ];
assert service.serviceConfig.User == "korri-portal";
assert service.serviceConfig.Group == "korri-portal";
assert service.serviceConfig.NoNewPrivileges;
# Input only through korrid: no device nodes, no input-group membership.
assert service.serviceConfig.PrivateDevices;
assert !(service.serviceConfig ? DeviceAllow);
assert !(service.serviceConfig ? SupplementaryGroups);
assert service.serviceConfig.KillMode == "control-group";
assert service.serviceConfig.RuntimeDirectory == "korri-portal";
assert service.serviceConfig.RuntimeDirectoryMode == "0700";
assert lib.hasPrefix "+" service.serviceConfig.ExecStartPre;
assert
  service.serviceConfig.BindReadOnlyPaths == [
    "/run/user/1001/korri-wayland:/run/korri-portal/korri-wayland"
  ];
assert service.environment.XDG_RUNTIME_DIR == "/run/korri-portal";
assert service.environment.WAYLAND_DISPLAY == "korri-wayland";
assert service.environment.HOME == "/var/lib/korri-portal";
assert !(service.environment ? DBUS_SESSION_BUS_ADDRESS);
assert !(service.environment ? KORRID_RPC_CAPABILITY);
assert !(daemon.environment ? KORRID_RPC_CAPABILITY);
assert builtins.elem credential service.serviceConfig.LoadCredential;
assert builtins.elem credential daemon.serviceConfig.LoadCredential;
# korrid binds every interface; the shell calls it on loopback.
assert service.environment.KORRID_ADDRESS == "127.0.0.1:39217";
assert service.environment.KORRID_SHELL_ORIGIN == daemon.environment.KORRID_SHELL_ORIGIN;
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
    credentialService
  ];
# The shell starts beside korrid, not after its network wait, and waits for
# korrid's socket itself.
assert builtins.elem "korrid.service" service.wantedBy;
assert builtins.elem "korrid.service" service.requires;
assert builtins.elem "korrid.service" service.partOf;
assert !(builtins.elem "korrid.service" service.after);
assert builtins.elem credentialService daemon.requires;
assert builtins.elem credentialService daemon.partOf;
# The device reads the summaries written at build time and never verifies.
assert builtins.elem "--installed" argv;
assert builtins.elem "${installed}/packages" argv;
assert builtins.elem "${installed}/cache" argv;
assert builtins.elem "pico,zone" argv;
assert
  builtins.elem "zone,pico" (lib.splitString " " started.systemd.services.korri-shell.serviceConfig.ExecStart);
pkgs.runCommand "korri-shell-module-check" { } ''
  grep -F 'User=korri-portal' ${pkgs.writeText "korri-shell.service" unit}
  grep -F 'Restart=on-failure' ${pkgs.writeText "korri-shell.service" unit}
  # Build-time install: one passing summary per package and the index.
  test -s ${installed}/cache/index.json
  test -s ${installed}/cache/surfaces/pico.json
  test -s ${installed}/cache/surfaces/zone.json
  grep -q '"pass": true' ${installed}/cache/surfaces/pico.json
  grep -q '"pass": true' ${installed}/cache/surfaces/zone.json
  export RUNTIME_DIRECTORY="$TMPDIR/credentials"
  mkdir -m 700 "$RUNTIME_DIRECTORY"
  ${producer.serviceConfig.ExecStart} > "$TMPDIR/credential-output"
  test ! -s "$TMPDIR/credential-output"
  test "$(stat -c %a "$RUNTIME_DIRECTORY/KORRID_RPC_CAPABILITY")" = 600
  grep -Eq '^[0-9a-f]{64}$' "$RUNTIME_DIRECTORY/KORRID_RPC_CAPABILITY"
  touch "$out"
''

# The native shell as the device's screen: one isolated system user draws the
# installed surface packages on the compositor's private Wayland socket and
# talks only to the korrid on this device: its own system user, a private
# bind of the compositor socket, and a per-boot capability through
# LoadCredential. It never runs as root and reads controller input only
# through korrid.
{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korri.shell;
  compositor = config.systemd.services.korri-compositor;
  display = compositor.environment.KORRI_WAYLAND_DISPLAY;
  system = pkgs.stdenv.hostPlatform.system;
  user = "korri-shell";
  group = user;
  home = "/var/lib/${user}";
  runtimeDir = "/run/${user}";
  unit = "korri-shell.service";
  # korrid checks this Origin on every RPC and the watch socket. Nothing
  # listens on it: it names the one local client korrid serves.
  origin = "http://127.0.0.1:8099";
  address = config.systemd.services.korrid.environment.KORRID_ADDRESS or "";
  addressPort = builtins.match ".*:([0-9]+)" address;
  # The shell always calls korrid over loopback, whatever korrid binds.
  shellAddress =
    if addressPort == null then
      throw "The shell requires korrid's configured socket address."
    else
      "127.0.0.1:${builtins.head addressPort}";
  credentialService = "korri-shell-credentials.service";
  credentialDirectory = "korri-shell-credentials";
  # The credential ID is the name korrid and the shell both read.
  credential = "KORRID_RPC_CAPABILITY:/run/${credentialDirectory}/KORRID_RPC_CAPABILITY";
  parents = [
    "korri-compositor.service"
    "korrid.service"
    credentialService
  ];
  # Start surface first, then every other installed package by id.
  installedIds = builtins.attrNames (
    lib.filterAttrs (_: kind: kind == "directory") (builtins.readDir ../packages)
  );
  surfaces =
    lib.optional (cfg.startSurface != null) cfg.startSurface
    ++ lib.filter (id: id != cfg.startSurface) installedIds;
  installed = korri.packages.${system}.korri-shell-surfaces;
  shell = korri.packages.${system}.korri-shell;
  grantWaylandAccess = pkgs.writeShellApplication {
    name = "korri-shell-wayland-access";
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
    name = "korri-shell-credentials";
    text = ''
      umask 077
      ${pkgs.openssl}/bin/openssl rand -hex 32 > "$RUNTIME_DIRECTORY/KORRID_RPC_CAPABILITY"
    '';
  };
in
{
  options.services.korri.shell = {
    enable = lib.mkEnableOption "the native Korri shell on the compositor";
    startSurface = lib.mkOption {
      type = lib.types.nullOr (lib.types.enum installedIds);
      default = null;
      description = ''
        Surface package the shell opens first. Null keeps package id order.
        A display preference, not a capability.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = addressPort != null;
        message = "The shell requires korrid's configured socket address.";
      }
      {
        assertion = builtins.baseNameOf display == display && display != "." && display != "..";
        message = "The shell requires the compositor's Wayland socket filename.";
      }
    ];
    users.groups.${group} = { };
    users.users.${user} = {
      isSystemUser = true;
      inherit group home;
    };
    systemd.services.korri-shell-credentials = {
      description = "Create the shell's korrid capability for this boot";
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
    # Both consumers restart when the producer rotates its credential.
    systemd.services.korrid = {
      environment.KORRID_SHELL_UNIT = unit;
      environment.KORRID_SHELL_ORIGIN = origin;
      wantedBy = [ credentialService ];
      requires = [ credentialService ];
      after = [ credentialService ];
      partOf = [ credentialService ];
      serviceConfig.LoadCredential = [ credential ];
    };
    systemd.services.korri-shell = {
      description = "Korri native shell";
      # A recovered parent must pull the shell back in after a failed boot job.
      wantedBy = [ "multi-user.target" ] ++ parents;
      requires = parents;
      # Not after korrid: korrid orders after network.target, and the screen
      # never waits for the network. The shell waits for korrid's socket.
      after = lib.remove "korrid.service" parents;
      partOf = parents;
      environment = {
        HOME = home;
        XDG_RUNTIME_DIR = runtimeDir;
        WAYLAND_DISPLAY = display;
        KORRID_ADDRESS = shellAddress;
        KORRID_SHELL_ORIGIN = origin;
      };
      serviceConfig = {
        Type = "exec";
        KillMode = "control-group";
        User = user;
        Group = group;
        ExecStartPre = "+${lib.getExe grantWaylandAccess}";
        ExecStart = lib.escapeShellArgs (
          [
            (lib.getExe' shell "korri-shell")
            "--packages"
            "${installed}/packages"
            "--cache"
            "${installed}/cache"
            "--installed"
            "--placement"
            "fullscreen"
          ]
          ++ lib.optionals (surfaces != [ ]) [
            "--surfaces"
            (lib.concatStringsSep "," surfaces)
          ]
        );
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
        # Software rendering over Wayland shared memory: no /dev/dri, and no
        # /dev/input. Controller input arrives from korrid.
        PrivateDevices = true;
        LimitCORE = 0;
        ReadWritePaths = [ runtimeDir ];
      };
    };
  };
}

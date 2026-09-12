{ korri }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.korriKiosk;
  host = config.services.korriLinuxHost;
  system = pkgs.stdenv.hostPlatform.system;
  runtime = "/run/user/${toString host.runtimeUid}";
  # Same stable socket published by korri-linux-host.nix. Game units hide
  # the whole per-user runtime directory and cannot connect to Wayland.
  chromium = pkgs.writeShellScript "korri-chromium" ''
    exec ${pkgs.chromium}/bin/chromium \
      --ozone-platform=wayland \
      --use-gl=angle --use-angle=gles-egl \
      --enable-gpu-rasterization --enable-zero-copy --ignore-gpu-blocklist \
      --class=korri-portal "$@"
  '';
in
{
  options.services.korriKiosk = {
    enable = lib.mkEnableOption "the private Chromium portal launcher";
    surfaceId = lib.mkOption {
      type = lib.types.str;
      default = "pico";
      description = "Runtime surface preference passed to the existing portal surface resolver.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = host.enable;
        message = "korriKiosk requires korriLinuxHost's compositor and device daemon.";
      }
      {
        assertion = config.services.static-web-server.listen == "127.0.0.1:8099";
        message = "The private kiosk interceptor trusts exactly http://127.0.0.1:8099/.";
      }
    ];
    services.korridLinuxDevice.browser = {
      enable = true;
      readGroup = host.runtimeGroup;
      origin = "http://127.0.0.1:8099";
    };
    # SWS is an existing Rust server. systemd retains the listening socket
    # when the server restarts, preventing takeover of the trusted origin.
    # The immutable portal contains no runtime.json or credentials.
    services.static-web-server = {
      enable = true;
      listen = "127.0.0.1:8099";
      root = korri.packages.x86_64-linux.korri-portal;
      configuration.general = {
        directory-listing = false;
        log-level = "warn";
      };
    };

    systemd.services.korri-kiosk = {
      description = "Korri private Chromium portal";
      wantedBy = [ "multi-user.target" ];
      requires = [
        "korrid.service"
        "static-web-server.service"
      ];
      bindsTo = [
        "korri-compositor.service"
        "static-web-server.socket"
        "korrid.service"
      ];
      partOf = [
        "korri-compositor.service"
        "korrid.service"
      ];
      after = [
        "korri-compositor.service"
        "korrid.service"
        "static-web-server.socket"
        "static-web-server.service"
      ];
      environment = {
        HOME = "/run/korri-kiosk";
        XDG_CONFIG_HOME = "/run/korri-kiosk";
        XDG_CACHE_HOME = "/run/korri-kiosk";
        XDG_RUNTIME_DIR = runtime;
        WAYLAND_DISPLAY = "korri-wayland";
        DBUS_SESSION_BUS_ADDRESS = "unix:path=${runtime}/bus";
      };
      serviceConfig = {
        User = host.runtimeUser;
        Group = host.runtimeGroup;
        SupplementaryGroups = [
          "video"
          "render"
        ];
        RuntimeDirectory = "korri-kiosk";
        RuntimeDirectoryMode = "0700";
        # Never remove the mounted exclusion beneath an already-running game.
        # The launcher removes stale profile children under its parent lock.
        RuntimeDirectoryPreserve = true;
        ExecStart = "${
          korri.packages.${system}.korri-kiosk
        }/bin/korri-kiosk --chromium ${chromium} --profile-parent /run/korri-kiosk --surface-id ${lib.escapeShellArg cfg.surfaceId}";
        KillMode = "control-group";
        TimeoutStopSec = 10;
        Restart = "on-failure";
        RestartSec = 2;
        LimitCORE = 0;
        UMask = "0077";
        NoNewPrivileges = true;
        CapabilityBoundingSet = [ ];
        AmbientCapabilities = [ ];
        ProtectSystem = "strict";
        ProtectHome = true;
        # ProtectHome=yes empties /home, /root *and* /run/user. The compositor
        # publishes its Wayland socket under the last one, so hiding it left
        # Chromium with "Failed to connect to Wayland display: Permission
        # denied" and a black panel. Keep the browser out of home directories
        # and bind back only the runtime directory it must reach.
        BindPaths = [ runtime ];
        ProtectProc = "invisible";
        ProcSubset = "pid";
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        PrivateTmp = true;
        ReadWritePaths = [
          "/run/korri-kiosk"
          runtime
        ];
      };
    };
  };
}

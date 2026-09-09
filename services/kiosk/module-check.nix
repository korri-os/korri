{ pkgs, korri }:
let
  lib = pkgs.lib;
  system = pkgs.stdenv.hostPlatform.system;
  evaluated = import "${pkgs.path}/nixos/lib/eval-config.nix" {
    inherit system;
    modules = [
      korri.nixosModules.korri-linux-host
      korri.nixosModules.korri-kiosk
      {
        system.stateVersion = "26.05";
        boot.loader.grub.enable = false;
        fileSystems."/" = {
          device = "none";
          fsType = "tmpfs";
        };
        users.groups.korri.gid = 1000;
        users.users.korri = {
          isNormalUser = true;
          uid = 1000;
          group = "korri";
          home = "/home/korri";
          linger = true;
        };
        services.korriLinuxHost = {
          enable = true;
          runtimeUser = "korri";
          runtimeUid = 1000;
          runtimeGroup = "korri";
          runtimeGid = 1000;
          relays = [ "ws://127.0.0.1:9" ];
          validation.enable = false;
          compositor.renderDevice = "/dev/dri/renderD128";
          sunshine.package = korri.packages.${system}.sunshine-korri;
        };
        services.korriBundle = {
          initialPackage = korri.packages.${system}.korri-bundle;
          launcherPackage = korri.packages.${system}.korri-inputd;
        };
        services.korriLinuxInput = {
          provider.package = korri.packages.${system}.inputplumber-korri;
          inputd.package = korri.packages.${system}.korri-inputd;
        };
        services.korriKiosk.enable = true;
      }
    ];
  };
  cfg = evaluated.config;
  kiosk = cfg.systemd.services.korri-kiosk;
in
assert lib.all (entry: entry.assertion) cfg.assertions;
assert cfg.services.static-web-server.listen == "127.0.0.1:8099";
assert cfg.services.korridLinuxDevice.browser.enable;
assert cfg.services.korridLinuxDevice.browser.readGroup == "korri";
assert builtins.elem "static-web-server.socket" kiosk.bindsTo;
assert builtins.elem "korri-compositor.service" kiosk.bindsTo;
assert kiosk.serviceConfig.RuntimeDirectoryMode == "0700";
assert kiosk.serviceConfig.RuntimeDirectoryPreserve;
assert builtins.elem "d /run/korri-kiosk 0700 korri korri -" cfg.systemd.tmpfiles.rules;
assert kiosk.serviceConfig.KillMode == "control-group";
assert kiosk.serviceConfig.LimitCORE == 0;
assert kiosk.serviceConfig.NoNewPrivileges;
assert kiosk.environment.XDG_RUNTIME_DIR == "/run/user/1000";
assert kiosk.environment.WAYLAND_DISPLAY == "korri-wayland";
assert !builtins.hasAttr "KORRID_RPC_CAPABILITY" kiosk.environment;
assert lib.hasInfix "--profile-parent /run/korri-kiosk" kiosk.serviceConfig.ExecStart;
pkgs.runCommand "korri-kiosk-module-check" { } ''
  touch "$out"
''

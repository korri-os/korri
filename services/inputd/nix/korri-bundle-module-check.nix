{
  pkgs,
  korriBundleModule,
  korriInputModule,
  korridLinuxDeviceModule,
  inputdPackage,
  inputplumberKorri,
  korridPackage,
  korriBundle,
}:
let
  deviceConfig = pkgs.writeText "korri-bundle-host.toml" ''
    label = "bundle-check"
  '';
  evaluated = import "${pkgs.path}/nixos/lib/eval-config.nix" {
    system = pkgs.stdenv.hostPlatform.system;
    modules = [
      korriBundleModule
      korriInputModule
      korridLinuxDeviceModule
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
        };
        services.korriBundle = {
          enable = true;
          initialPackage = korriBundle;
          launcherPackage = inputdPackage;
        };
        services.korriLinuxInput = {
          provider = {
            enable = true;
            package = inputplumberKorri;
          };
          inputd = {
            enable = true;
            package = inputdPackage;
            uid = 977;
            controlGid = 977;
            actionUser = "korri";
            actionUid = 1000;
            actionGid = 1000;
          };
        };
        services.korridLinuxDevice = {
          enable = true;
          package = korridPackage;
          uid = 976;
          gid = 976;
          runtimeUser = "korri";
          runtimeUid = 1000;
          runtimeGid = 1000;
          inputdUid = 977;
          controlGid = 977;
          inherit deviceConfig;
          sunshinePrivateStateRoot = "/home/korri/.config/sunshine";
        };
      }
    ];
  };
  selector = evaluated.config.systemd.services.korri-bundle-selector;
  provider = evaluated.config.systemd.services.inputplumber;
  inputd = evaluated.config.systemd.services.korri-inputd;
  korrid = evaluated.config.systemd.services.korrid;
in
assert provider.serviceConfig.ExecStart == "${inputdPackage}/bin/korri-bundle-launch inputplumber";
assert inputd.serviceConfig.ExecStart == "${inputdPackage}/bin/korri-bundle-launch inputd";
assert korrid.serviceConfig.ExecStart == "${inputdPackage}/bin/korri-bundle-launch korrid";
assert builtins.elem "korri-bundle-selector.service" provider.requires;
assert builtins.elem "korri-bundle-selector.service" inputd.requires;
assert builtins.elem "korri-bundle-selector.service" korrid.requires;
assert
  selector.serviceConfig.ExecStart
  == "${inputdPackage}/bin/korri-bundle-select initialize ${korriBundle}";
pkgs.runCommand "korri-bundle-module-check" { } ''
  touch "$out"
''

{
  pkgs,
  cacheModule ? { },
}:
let
  evaluate =
    modules:
    (import "${pkgs.path}/nixos/lib/eval-config.nix" {
      inherit (pkgs.stdenv.hostPlatform) system;
      modules = [ { system.stateVersion = "26.05"; } ] ++ modules;
    }).config;
  builder = evaluate [ ];
  device = evaluate [ cacheModule ];
  conflicting = evaluate [
    cacheModule
    {
      nix.distributedBuilds = true;
      nix.settings = {
        max-jobs = 8;
        builders = "ssh://builder x86_64-linux";
        fallback = true;
        require-sigs = false;
      };
    }
  ];
  inherit (device.nix) settings;
in
assert builder.nix.settings.max-jobs != 0;
assert settings.max-jobs == 0;
assert settings.builders == "";
assert !device.nix.distributedBuilds;
assert !settings.fallback;
assert settings.require-sigs;
assert conflicting.nix.settings.max-jobs == 0;
assert conflicting.nix.settings.builders == "";
assert !conflicting.nix.distributedBuilds;
assert !conflicting.nix.settings.fallback;
assert conflicting.nix.settings.require-sigs;
assert settings.always-allow-substitutes;
assert builtins.elem "https://cache.nixos.org/" settings.substituters;
assert builtins.elem "https://cache.garnix.io" settings.substituters;
assert builtins.elem "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY="
  settings.trusted-public-keys;
assert builtins.elem "cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g="
  settings.trusted-public-keys;
pkgs.runCommand "korri-device-cache-module-check"
  {
    nativeBuildInputs = [
      pkgs.nix
      pkgs.python3
    ];
  }
  ''
    python3 ${./cache-check.py} \
      ${device.environment.etc."nix/nix.conf".source} \
      ${./build-fixture.nix} \
      ${pkgs.runtimeShell} \
      ${pkgs.stdenv.hostPlatform.system}
    touch "$out"
  ''

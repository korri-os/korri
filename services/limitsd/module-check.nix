{
  pkgs,
  nixpkgs,
  korri,
}:
let
  inherit (pkgs) lib;
  system = pkgs.stdenv.hostPlatform.system;
  evaluated = nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [
      ../../nix/base/clock-governor.nix
      (import ./nixos-module.nix { inherit korri; })
      {
        options.services.korridLinuxDevice.uid = lib.mkOption {
          type = lib.types.int;
          default = 976;
        };
        config.services.korri = {
          limits.enable = true;
          clockGovernor = {
            enable = true;
            gpuDevfreqNodes = [ "gpu" ];
            cpuIdleDisable = [ "cpu0/cpuidle/state1" ];
          };
        };
      }
    ];
  };
  c = evaluated.config;
  defaults = builtins.fromJSON c.systemd.services.korri-limits.environment.LIMITSD_DEFAULTS;
in
assert
  defaults == {
    cpu = {
      min = 0;
      max = 100;
    };
    gpu = {
      min = 0;
      max = 100;
    };
    governor = "schedutil";
  };
assert c.services.korri.limits.socketPath == "/run/korri-limits/limits.sock";
assert c.systemd.sockets.korri-limits.socketConfig.SocketMode == "0660";
assert c.systemd.sockets.korri-limits.socketConfig.SocketGroup == "korrid";
assert c.systemd.services.korri-limits.environment.LIMITSD_PEER_UID == "976";
assert c.systemd.services.korri-limits.environment.LIMITSD_GPU_NODES == "gpu";
assert c.systemd.slices.korri-games.sliceConfig.CPUWeight == 100;
assert c.systemd.slices.korri.sliceConfig.CPUWeight == 100;
assert c.systemd.slices.korri.sliceConfig.CPUAccounting;
assert
  korri.nixosConfigurations.rg353m.config.services.korri.clockGovernor.gpuDevfreqNodes
  == [ "fde60000.gpu" ];
assert
  korri.nixosConfigurations.rg353m.config.services.korri.limits.gpuDevfreqNodes == [ "fde60000.gpu" ];
assert korri.nixosConfigurations.rg353m.config.services.korri.limits.defaults.cpu.max == 61;
assert
  korri.nixosConfigurations.rg353m.config.systemd.services.korri-limits.environment.LIMITSD_GPU_NODES
  == "fde60000.gpu";
assert
  korri.nixosConfigurations.rg353m.config.systemd.services.korri-clock-governor.environment.KORRI_CLOCK_GPU_NODES
  == "fde60000.gpu";
pkgs.runCommand "korri-limits-module-check" { } ''
  root="$TMPDIR/sys"
  cpu="$root/devices/system/cpu/cpufreq/policy0"
  gpu="$root/class/devfreq/gpu"
  idle="$root/devices/system/cpu/cpu0/cpuidle/state1"
  mkdir -p "$cpu" "$gpu" "$idle"
  echo performance > "$cpu/scaling_governor"
  echo 'performance schedutil' > "$cpu/scaling_available_governors"
  echo 400 > "$cpu/scaling_min_freq"
  echo 1800 > "$cpu/scaling_max_freq"
  echo performance > "$gpu/governor"
  echo 'performance simple_ondemand' > "$gpu/available_governors"
  echo 0 > "$idle/disable"
  env KORRI_CLOCK_SYSFS_ROOT="$root" KORRI_CLOCK_MAX_ITERATIONS=1 \
    KORRI_CLOCK_GPU_NODES=gpu KORRI_CLOCK_CPUIDLE_DISABLE=cpu0/cpuidle/state1 \
    ${c.systemd.services.korri-clock-governor.serviceConfig.ExecStart}
  test "$(cat "$cpu/scaling_governor")" = performance
  test "$(cat "$cpu/scaling_min_freq")" = 400
  test "$(cat "$cpu/scaling_max_freq")" = 1800
  test "$(cat "$gpu/governor")" = simple_ondemand
  test "$(cat "$idle/disable")" = 1
  touch "$out"
''

# Device policy only. CI and development machines must not import this module.
{ lib, ... }:
{
  nix.distributedBuilds = lib.mkForce false;
  nix.settings = {
    max-jobs = lib.mkForce 0;
    builders = lib.mkForce "";
    fallback = lib.mkForce false;
    require-sigs = lib.mkForce true;
    # Small Nix outputs can declare allowSubstitutes = false. Devices must
    # fetch these outputs too, rather than build wrappers or symlink trees.
    always-allow-substitutes = true;
    # Published by https://garnix.io/docs/ci/caching/.
    # NixOS appends cache.nixos.org and its key in its own Nix module.
    substituters = [ "https://cache.garnix.io" ];
    trusted-public-keys = [
      "cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g="
    ];
  };
}

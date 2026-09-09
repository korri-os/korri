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
    # Keep NixOS's stock cache and key. Plugin release archives are not Nix
    # substituters; the host verifies and stages them as local file caches.
  };
}

# Preserve the real locked Nixpkgs identity without retaining its source tree.
# `to` is Nix's native fetcher-attribute format, also used by flake.lock.
{
  config,
  lib,
  korri,
  ...
}:
let
  locked = (builtins.fromJSON (builtins.readFile ../../../flake.lock)).nodes.nixpkgs.locked;
in
{
  assertions = [
    {
      assertion =
        locked.rev == korri.inputs.nixpkgs.rev && locked.narHash == korri.inputs.nixpkgs.narHash;
      message = "RG353M's Nixpkgs registry must match the actual locked input; update flake.lock before building.";
    }
  ];
  # Override the upstream path-based default as a whole, so no source path
  # survives a recursive attribute merge. Keep the existing NIX_PATH alias.
  nix.registry.nixpkgs.to = lib.mkForce locked;
  system.systemBuilderArgs.disallowedRequisites = [ config.nixpkgs.flake.source ];
}

# Use RG353M's locked native fetcher record instead of a bundled source tree.
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
      message = "R36T Max's Nixpkgs registry must match the actual locked input; update flake.lock before building.";
    }
  ];
  # Replace the whole record so the upstream path field cannot survive merging.
  # Keep Nix, its NIX_PATH alias and the download-only device policy unchanged.
  nix.registry.nixpkgs.to = lib.mkForce locked;
  system.systemBuilderArgs.disallowedRequisites = [ config.nixpkgs.flake.source ];
}

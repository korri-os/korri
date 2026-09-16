# One catalogue, one package per core.
{ pkgs, mkPlugin }:
let
  lib = pkgs.lib;
  catalogue = import ./cores.nix { inherit pkgs; };
  mkCorePlugin = import ./core-plugin.nix { inherit pkgs mkPlugin; };
in
{
  inherit catalogue;
  packages = lib.mapAttrs' (
    name: spec: lib.nameValuePair "korri-plugin-${name}" (mkCorePlugin name spec)
  ) catalogue;
}

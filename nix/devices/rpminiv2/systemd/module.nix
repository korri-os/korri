# Product board edge only. Do not import into recovery or shared product modules.
{ pkgs, ... }:
{
  systemd.package = pkgs.callPackage ./package.nix { };
}

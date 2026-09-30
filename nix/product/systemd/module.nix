# Product systemd: stop thaws a frozen unit (approved 2026-09-26 for the Mini V2,
# made product-wide 2026-09-30). The portal freeze during games needs it.
{ pkgs, ... }:
{
  systemd.package = pkgs.callPackage ./package.nix { };
}

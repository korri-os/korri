# Stock-firmware SSH reconnaissance only. Standalone NixOS belongs to
# nix/devices/r36tmax and is exported through the repository flake.
{ nixpkgs }:
{
  dropbearStatic = nixpkgs.legacyPackages.aarch64-linux.pkgsStatic.dropbear;
}

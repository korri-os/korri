# REVIEW EXAMPLE. Ordinary Nix helper, not a NixOS module.
# Uses an explicit nixpkgs pin/package choice without abandoning the helper.
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    retroarch.url = "github:korri-os/retroarch";
    retroarch.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs =
    { nixpkgs, retroarch, ... }:
    {
      packages = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = retroarch.lib.mkCorePlugin {
            inherit pkgs;
            namespace = "@local";
            name = "mgba";
            core = pkgs.libretro.mgba;
            core-file = "${pkgs.libretro.mgba}/lib/retroarch/cores/mgba_libretro.so";
            systems.gba = {
              title = "Game Boy Advance";
              extensions = [ "gba" ];
            };
            # Optional. May instead be a pinned fork or overrideAttrs derivation.
            # It is already a derivation, not a reference to an installed plugin.
            frontend = pkgs.retroarch-bare;
            # The helper must tie evidence/config support to this selected binary.
          };
        }
      );
    };
}

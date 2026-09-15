# REVIEW EXAMPLE. The builder API below is proposed, not published.
# In the real RetroArch plugin repo this helper would be its own source.
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
          catalogue = import ./cores.nix { inherit pkgs; };
        in
        pkgs.lib.mapAttrs (
          name: spec:
          retroarch.lib.mkCorePlugin {
            inherit pkgs name;
            namespace = "@korri";
            inherit (spec) core core-file systems;
            source = spec.plugin or null;
          }
        ) catalogue
      );
    };
}

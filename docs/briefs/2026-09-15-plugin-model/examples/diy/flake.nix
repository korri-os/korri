# REVIEW EXAMPLE. No RetroArch dependency. Builder API is proposed.
# A hand-written builder may emit the same manifest/source layout.
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    plugin-lib.url = "github:korri-os/plugin-lib";
    plugin-lib.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs =
    { nixpkgs, plugin-lib, ... }:
    {
      packages = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ] (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = plugin-lib.lib.mkPlugin {
            inherit pkgs;
            namespace = "@local";
            source = ./.;
            files.program = "${pkgs.ppsspp-sdl}/bin/PPSSPPSDL";
          };
        }
      );
    };
}

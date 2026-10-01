# Anbernic RG35XX Pro: the H700 family product with this board's facts.
{ nixpkgs, korri }:
let
  h700 = import ../h700 { inherit nixpkgs korri; };
  configuration = h700.mkConfiguration {
    device = "rg35xxpro";
    module = ./hardware.nix;
  };
in
{
  inherit configuration;
  sdImage = configuration.config.system.build.sdImage;
}

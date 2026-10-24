{
  pkgs,
  crane,
  proseql,
  buildIdentity ? null,
}:
let
  package = import ./package.nix {
    inherit
      pkgs
      crane
      proseql
      buildIdentity
      ;
  };
in
{
  inherit package;
  apps.korri = {
    type = "app";
    program = "${package}/bin/korri";
  };
}

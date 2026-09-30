# Supported build-side plugin interface. No host binary or host test enters
# plugin package construction. Callers own the package set they pass in.
let
  mkPlugin = import ./builder.nix;
in
{
  inherit mkPlugin;
  pluginContract = builtins.path {
    path = ../../../contracts/generated/korrid.ts;
    name = "korrid.ts";
  };
  pluginBuilderCheck =
    { pkgs }:
    import ./builder-check.nix {
      inherit pkgs;
      mkPlugin = mkPlugin { inherit pkgs; };
    };
}

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
  # korrid.ts imports the surface treaty's types from ./surface: place this
  # file beside it as surface.ts.
  pluginSurfaceContract = builtins.path {
    path = ../../../contracts/generated/surface.ts;
    name = "surface.ts";
  };
  pluginBuilderCheck =
    { pkgs }:
    import ./builder-check.nix {
      inherit pkgs;
      mkPlugin = mkPlugin { inherit pkgs; };
    };
}

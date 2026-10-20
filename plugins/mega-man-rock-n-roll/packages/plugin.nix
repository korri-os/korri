# Local build entry until signed publication supplies an immutable image pin.
# The required FEX plugin is built through the same builder, so this
# manifest names its exact output.
{ pkgs }:
(import ../../../services/korrid/plugin-host/builder.nix { inherit pkgs; }) {
  publisher.namespace = "@korri";
  source = pkgs.lib.cleanSourceWith {
    src = ../.;
    filter = path: type: type == "directory" || baseNameOf path == "plugin.ts";
  };
  plugin =
    _:
    import ../plugin.nix {
      inherit pkgs;
      fexPlugin = import ../../fex/packages/plugin.nix { inherit pkgs; };
    };
}

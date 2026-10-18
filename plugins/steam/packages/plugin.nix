# Local build entry until signed publication supplies an immutable image pin.
# The required FEX and Proton plugins are built through the same builder, so
# Steam's manifest names their exact outputs.
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
      protonPlugin = import ../../proton/packages/plugin.nix { inherit pkgs; };
    };
}

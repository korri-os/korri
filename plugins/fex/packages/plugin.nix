# Local build entry until signed publication supplies an immutable image pin.
{ pkgs }:
(import ../../../services/korrid/plugin-host/builder.nix { inherit pkgs; }) {
  publisher.namespace = "@korri";
  source = pkgs.lib.cleanSourceWith {
    src = ../.;
    filter = path: type: type == "directory" || baseNameOf path == "plugin.ts";
  };
  plugin = ../plugin.nix;
}

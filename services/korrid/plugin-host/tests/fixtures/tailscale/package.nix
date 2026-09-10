{ pkgs }:
(import ../../../builder.nix { inherit pkgs; }) {
  publisher.namespace = "@korri";
  source = ./plugin.ts;
  plugin = ./plugin.nix;
}

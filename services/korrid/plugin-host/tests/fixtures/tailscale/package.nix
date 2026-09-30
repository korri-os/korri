{ pkgs }:
((import ../../../interface.nix).mkPlugin { inherit pkgs; }) {
  publisher.namespace = "@korri";
  source = ./.;
  plugin = ./plugin.nix;
}

# Unpublished experiment. The plugin host still needs a publisher-bound signature
# and an exact new approval before it can select this changed closure on a device.
{
  system ? "aarch64-linux",
  forceOff ? false,
}:
let
  flake = builtins.getFlake ("path:" + toString ../..);
  pluginBuilder = flake.lib.${system}.mkPlugin;
  sunshinePackage = import ./rotation-probe-build.nix { inherit system forceOff; };
in
assert system == "aarch64-linux";
pluginBuilder {
  publisher.namespace = "@korri";
  source = ../../plugins/sunshine;
  plugin = { pkgs }: import ../../plugins/sunshine/plugin.nix {
    inherit pkgs sunshinePackage;
  };
}

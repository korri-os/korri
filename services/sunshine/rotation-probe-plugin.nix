# Unpublished experiment. The plugin host still needs a publisher-bound signature
# and an exact new approval before it can select this changed closure on a device.
{
  system ? "aarch64-linux",
  forceOff ? false,
  korri ? builtins.getFlake ("path:" + toString ../..),
}:
let
  pluginBuilder = korri.lib.${system}.mkPlugin;
  sunshinePackage = import ./rotation-probe-build.nix { inherit system forceOff korri; };
in
# The official publisher builds the same selected names on both architectures.
# The Mini trial uses only the aarch64-linux outputs.
assert builtins.elem system [ "x86_64-linux" "aarch64-linux" ];
pluginBuilder {
  publisher.namespace = "@korri";
  source = ../../plugins/sunshine;
  plugin = { pkgs }: import ../../plugins/sunshine/plugin.nix {
    inherit pkgs sunshinePackage;
  };
}

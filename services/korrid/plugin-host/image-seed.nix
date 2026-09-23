# The NixOS SD builder copies storePaths into the image separately from the
# system closure. Only the generated receipt and GC root retain a selected
# plugin on the device after the image has been written.
{
  pkgs,
  hostPackage,
  cacheUrl,
  pluginPackages,
}:
let
  seed = pkgs.writeShellApplication {
    name = "korri-image-seed";
    runtimeInputs = [ pkgs.coreutils pkgs.jq ];
    text = builtins.readFile ./image-seed.sh;
  };
in
{
  storePaths = pluginPackages;
  populateRootCommands = ''
    ${seed}/bin/korri-image-seed ./files ${hostPackage}/bin/korri-plugin \
      ${pkgs.lib.escapeShellArg cacheUrl} \
      ${pkgs.lib.escapeShellArgs (map toString pluginPackages)}
  '';
}

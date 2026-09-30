# The NixOS SD builder copies storePaths into the image separately from the
# system closure. Receipts and GC roots retain active selections. The offline
# proof service also retains default closures in the system generation.
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

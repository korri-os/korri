# The NixOS SD builder copies storePaths into the image separately from the
# system closure. Receipts and GC roots retain active selections. The offline
# proof service also retains default closures in the system generation.
{
  pkgs,
  hostPackage,
  publishers,
  pluginPackages,
  disabled ? [ ],
  catalog ? null,
}:
let
  bindings = pkgs.writeText "korri-image-publishers.json" (builtins.toJSON publishers);
  seed = pkgs.writeShellApplication {
    name = "korri-image-seed";
    runtimeInputs = [
      pkgs.coreutils
      pkgs.jq
    ];
    text = builtins.readFile ./image-seed.sh;
  };
in
{
  storePaths = pluginPackages;
  populateRootCommands = ''
    ${seed}/bin/korri-image-seed ./files ${hostPackage}/bin/korri-plugin \
      ${
        pkgs.lib.escapeShellArgs (
          pkgs.lib.concatMap (id: [
            "--disabled"
            id
          ]) disabled
        )
      } \
      ${bindings} \
      ${pkgs.lib.optionalString (catalog != null) ("--catalog " + pkgs.lib.escapeShellArgs [ catalog.catalog catalog.catalogUrl ])} \
      ${pkgs.lib.escapeShellArgs (map toString pluginPackages)}
  '';
}

# Current Core acceptance over immutable published packages. No plugin builder
# or publisher source enters this derivation: roots are exact store references.
{
  pkgs,
  system,
  hostPackage,
  publicKey,
  cacheUrl,
}:
let
  published = import ./published-plugins.nix { inherit system; };
  metadata = import ./published-plugin-metadata.nix {
    inherit pkgs system publicKey;
    pluginPackages = builtins.attrValues published;
  };
  proofCheck = import ./published-plugin-metadata-check.nix {
    inherit
      pkgs
      system
      publicKey
      published
      metadata
      ;
  };
  identityChecks = pkgs.lib.concatStringsSep "\n" (
    pkgs.lib.mapAttrsToList (name: package: ''
      ${hostPackage}/bin/korri-plugin seed ${pkgs.lib.escapeShellArg package} ${pkgs.lib.escapeShellArg cacheUrl} \
        | jq -e --arg id ${
          pkgs.lib.escapeShellArg ("@korri:" + pkgs.lib.removePrefix "korri-plugin-" name)
        } \
            --arg path ${pkgs.lib.escapeShellArg package} \
            '.id == $id and .package == $path and .desired.state == "Enabled" and .previous == null' >/dev/null
    '') published
  );
in
assert pkgs.lib.all (
  package:
  builtins.getContext package == {
    "${builtins.unsafeDiscardStringContext package}".path = true;
  }
) (builtins.attrValues published);
pkgs.runCommand "korri-published-plugin-acceptance-${system}" { nativeBuildInputs = [ pkgs.jq ]; }
  ''
    set -euo pipefail
    test -f ${metadata}/nix-cache-info
    test -e ${proofCheck}
    ${identityChecks}
    touch "$out"
  ''

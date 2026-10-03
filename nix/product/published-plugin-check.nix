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
  publishers = pkgs.writeText "published-plugin-publishers.json" (
    builtins.toJSON { "@korri" = { inherit publicKey cacheUrl; }; }
  );
  identityChecks = pkgs.lib.concatStringsSep "\n" (
    pkgs.lib.mapAttrsToList (name: package: ''
      jq -e --arg id ${
        pkgs.lib.escapeShellArg ("@korri:" + pkgs.lib.removePrefix "korri-plugin-" name)
      } --arg path ${pkgs.lib.escapeShellArg package} \
        '.[] | select(.id == $id) | .package == $path and .desired.state == "Enabled" and .previous == null' \
        receipts.json >/dev/null
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
    ${hostPackage}/bin/korri-plugin seed-graph ${publishers} \
      ${pkgs.lib.escapeShellArgs (builtins.attrValues published)} > receipts.json
    jq -e 'length == ${toString (builtins.length (builtins.attrNames published))}' receipts.json >/dev/null
    ${identityChecks}
    # Image selection must approve the exact dependency the pack names.
    # Keeping another FAKE-08 version in the store is not sufficient.
    jq -e --arg dependency ${pkgs.lib.escapeShellArg published.korri-plugin-fake08} \
      '.publisher.namespace == "@korri" and .requires == [$dependency]' \
      ${published.korri-plugin-starter-pack}/manifest.json >/dev/null
    touch "$out"
  ''

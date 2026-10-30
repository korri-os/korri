# Admission of actual catalog outputs, never recipes rebuilt with Core.
{
  pkgs,
  system,
  hostPackage,
  seedPackage,
  publicKey,
  cacheUrl,
}:
let
  lib = pkgs.lib;
  catalog = import ./catalog-plugins.nix { inherit pkgs system; };
  published = catalog.plugins;
  publishers = {
    "@korri" = { inherit publicKey cacheUrl; };
  };
  image = import ../../services/korrid/plugin-host/image-seed.nix {
    inherit pkgs publishers catalog;
    hostPackage = seedPackage;
    pluginPackages = builtins.attrValues published;
    disabled = [ "@korri:starter-pack" ];
  };
  metadata = import ./published-plugin-metadata.nix {
    inherit
      pkgs
      system
      hostPackage
      publishers
      ;
    pluginPackages = builtins.attrValues published;
  };
  proofCheck = import ./published-plugin-metadata-check.nix {
    inherit pkgs system metadata;
    pluginPackages = builtins.attrValues published;
  };
  identityChecks = lib.concatStringsSep "\n" (
    lib.mapAttrsToList (name: record: ''
      jq -e --argjson record ${lib.escapeShellArg (builtins.toJSON record)} \
        --arg source ${lib.escapeShellArg catalog.catalogUrl} \
        '.[] | select(.id == $record.plugin_id) |
          .package == $record.store_path and .previous == null and
          .desired.state == "${if name == "korri-plugin-starter-pack" then "Disabled" else "Enabled"}" and
          .provenance == {kind: "Repository", source_url: $source,
            plugin_id: $record.plugin_id, release_version: $record.release_version,
            platform: $record.platform, archive_sha256: $record.archive_sha256, signed: true}' \
        receipts.json >/dev/null
    '') catalog.records
  );
in
assert lib.all (
  package:
  builtins.getContext package == {
    "${builtins.unsafeDiscardStringContext package}".path = true;
  }
) (builtins.attrValues published);
pkgs.runCommand "korri-published-plugin-acceptance-${system}" { nativeBuildInputs = [ pkgs.jq ]; }
  ''
    set -euo pipefail
    test -e ${proofCheck}
    ${image.populateRootCommands}
    jq -s . files/var/lib/korri-plugin-host/*/selection.json > receipts.json
    for receipt in files/var/lib/korri-plugin-host/*/selection.json; do
      test "$(stat -c %a "$receipt")" = 600
      test "$(stat -c %a "$(dirname "$receipt")")" = 700
    done
    test "$(find files/nix/var/nix/gcroots/korri-plugin-host -name active -type l | wc -l)" = 33
    jq -e 'length == 33' receipts.json >/dev/null
    ${identityChecks}
    jq -e --arg fake08 ${lib.escapeShellArg published.korri-plugin-fake08} \
      --arg solarus ${lib.escapeShellArg published.korri-plugin-solarus} \
      --arg zquest ${lib.escapeShellArg published.korri-plugin-zquest-classic} \
      '.publisher.namespace == "@korri" and .requires == [$fake08, $solarus, $zquest]' \
      ${published.korri-plugin-starter-pack}/manifest.json >/dev/null
    mkdir -p "$out"
    cp receipts.json "$out/receipts.json"
  ''

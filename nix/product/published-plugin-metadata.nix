# Signed proofs come from the selected catalog archives, not raw-cache metadata.
# Nix verifies them against an unsigned image registration. Archives stay in the
# generation for offline recovery; only their small narinfo caches stay unpacked.
{
  pkgs,
  system,
  pluginPackages,
  hostPackage,
  publishers,
}:
let
  lib = pkgs.lib;
  published = import ./catalog-plugins.nix { inherit pkgs system; };
  names = builtins.filter (name: lib.elem published.plugins.${name} pluginPackages) (
    builtins.attrNames published.plugins
  );
  bindings = pkgs.writeText "korri-image-publishers.json" (builtins.toJSON publishers);
  closure = pkgs.closureInfo { rootPaths = pluginPackages; };
  inputs = lib.concatMapStringsSep " \\\n" (
    name: "${lib.escapeShellArg published.records.${name}.plugin_id} \"$base/${name}\""
  ) names;
in
pkgs.runCommand "korri-image-plugin-offline-metadata"
  {
    nativeBuildInputs = [
      pkgs.buildPackages.nix
      pkgs.buildPackages.coreutils
    ];
  }
  ''
    mkdir -p "$out"
    ${lib.concatMapStringsSep "\n" (name: ''
      ${hostPackage}/bin/korri-plugin image-extract ${published.archives.${name}} \
        ${lib.escapeShellArg published.records.${name}.archive_sha256} "$out/${name}"
      ln -s ${published.archives.${name}} "$out/${name}.tar"
    '') names}
    cat > "$out/verify" <<'SCRIPT'
    #!${pkgs.runtimeShell}
    set -euo pipefail
    base="$(${pkgs.coreutils}/bin/dirname "$(${pkgs.coreutils}/bin/readlink -f "$0")")"
    exec ${hostPackage}/bin/korri-plugin image proofs ${bindings} \
      ${
        lib.escapeShellArgs [
          system
          published.catalog
          published.catalogUrl
        ]
      } \
      ${inputs}
    SCRIPT
    chmod +x "$out/verify"
    export NIX_REMOTE="local?state=$TMPDIR/image-state"
    nix-store --load-db < ${closure}/registration
    "$out/verify"
    # NAR payloads already exist in the registered image closure and the retained
    # pinned archives. copy-sigs needs only the original signed narinfos.
    ${lib.concatMapStringsSep "\n" (name: ''rm -r "$out/${name}/nar"'') names}
  ''

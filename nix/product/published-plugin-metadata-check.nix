# Run on the build host, including when proofs describe foreign-architecture
# packages. Only native Nix reads those packages; no plugin program is executed.
{
  pkgs,
  system,
  publicKey,
  published ? import ./published-plugins.nix { inherit system; },
  metadata ? import ./published-plugin-metadata.nix {
    inherit pkgs system publicKey;
    pluginPackages = builtins.attrValues published;
  },
}:
let
  pluginPackages = builtins.attrValues published;
  closure = pkgs.closureInfo { rootPaths = pluginPackages; };
in
pkgs.runCommand "korri-published-plugin-metadata-check-${system}"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.nix
    ];
  }
  ''
    cp ${./published-plugin-proofs.py} published-plugin-proofs.py
    cp ${./published-plugin-proofs-test.py} published-plugin-proofs-test.py
    python3 published-plugin-proofs-test.py \
      --system ${pkgs.lib.escapeShellArg system} \
      --metadata ${metadata} \
      --closure ${closure}/store-paths \
      --registration ${closure}/registration \
      --public-key ${pkgs.lib.escapeShellArg publicKey} \
      --roots ${pkgs.lib.escapeShellArgs pluginPackages} \
      --archives ${pkgs.lib.escapeShellArgs metadata.archives}
    touch "$out"
  ''

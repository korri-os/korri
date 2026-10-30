{
  pkgs,
  system,
  metadata,
  pluginPackages,
}:
let
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
    python3 ${./published-plugin-proofs-test.py} ${metadata} ${closure}/registration \
      ${pkgs.lib.escapeShellArgs pluginPackages}
    touch "$out"
  ''

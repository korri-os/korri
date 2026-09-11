# Check the emitted native registry, not a second hand-maintained version pin.
{ pkgs, configuration }:
let
  cfg = configuration.config;
  lock = (builtins.fromJSON (builtins.readFile ../../../flake.lock)).nodes.nixpkgs.locked;
  registry = cfg.environment.etc."nix/registry.json".source;
in
assert cfg.nix.registry.nixpkgs.to == lock;
assert cfg.nix.registry.nixpkgs.exact;
assert builtins.elem "nixpkgs=flake:nixpkgs" cfg.nix.nixPath;
assert builtins.elem cfg.nixpkgs.flake.source cfg.system.systemBuilderArgs.disallowedRequisites;
pkgs.runCommand "rg353m-nixpkgs-registry-check"
  {
    nativeBuildInputs = [
      pkgs.jq
      pkgs.nix
    ];
  }
  ''
    jq -e --argjson expected '${builtins.toJSON lock}' '
      .version == 2 and
      ([.flakes[] | select(.from == {"type":"indirect","id":"nixpkgs"})] |
        length == 1 and .[0].exact == true and .[0].to == $expected and
        (.[0].to | has("path") | not))
    ' ${registry}
    # Exercise the real Nix registry reader in an isolated configuration, without
    # fetching anything or using the build host's registry/preferences.
    export HOME="$TMPDIR/home"
    export NIX_CONF_DIR="$TMPDIR/nix-conf"
    export NIX_USER_CONF_FILES=/dev/null
    mkdir -p "$HOME" "$NIX_CONF_DIR"
    cp ${registry} "$NIX_CONF_DIR/registry.json"
    nix --extra-experimental-features 'nix-command flakes' registry list \
      --store dummy:// --option flake-registry "" > registry-list.txt
    grep -F '${lock.rev}' registry-list.txt
    if grep -F '${cfg.nixpkgs.flake.source}' registry-list.txt; then
      echo 'Registry still points at the source store path' >&2
      exit 1
    fi
    touch "$out"
  ''

# RG353M's native-registry check, applied to every R36T Max image variant.
{ pkgs, configurations }:
let
  lock = (builtins.fromJSON (builtins.readFile ../../../flake.lock)).nodes.nixpkgs.locked;
  valid =
    configuration:
    let
      cfg = configuration.config;
    in
    cfg.nix.registry.nixpkgs.to == lock
    && cfg.nix.registry.nixpkgs.exact
    && builtins.elem "nixpkgs=flake:nixpkgs" cfg.nix.nixPath
    && builtins.elem cfg.nixpkgs.flake.source cfg.system.systemBuilderArgs.disallowedRequisites;
  registry = (builtins.head configurations).config.environment.etc."nix/registry.json".source;
in
assert pkgs.lib.assertMsg (pkgs.lib.all valid configurations)
  "R36T Max images must fetch the locked Nixpkgs source on demand";
pkgs.runCommand "r36tmax-nixpkgs-registry-check"
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
    export HOME="$TMPDIR/home"
    export NIX_CONF_DIR="$TMPDIR/nix-conf"
    export NIX_USER_CONF_FILES=/dev/null
    mkdir -p "$HOME" "$NIX_CONF_DIR"
    cp ${registry} "$NIX_CONF_DIR/registry.json"
    nix --extra-experimental-features 'nix-command flakes' registry list \
      --store dummy:// --option flake-registry "" > registry-list.txt
    grep -F '${lock.rev}' registry-list.txt
    if grep -F '/nix/store/' registry-list.txt; then
      echo 'Registry still points at a store path' >&2
      exit 1
    fi
    touch "$out"
  ''

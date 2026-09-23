{
  pkgs,
  hostPackage,
  sshPackage,
}:
let
  image = import ./image-seed.nix {
    inherit pkgs hostPackage;
    cacheUrl = "https://cache.example.invalid";
    pluginPackages = [ sshPackage ];
  };
in
assert builtins.length image.storePaths == 1;
pkgs.runCommand "korri-plugin-image-seed-check" { nativeBuildInputs = [ pkgs.coreutils pkgs.jq ]; } ''
  mkdir -p files
  ${image.populateRootCommands}
  receipt=$(find files/var/lib/korri-plugin-host -name selection.json -type f)
  test "$(jq -er .id "$receipt")" = '@korri:ssh'
  test "$(jq -er .package "$receipt")" = '${sshPackage}'
  test "$(jq -er .desired.state "$receipt")" = Enabled
  test "$(jq -r .previous "$receipt")" = null
  name=$(basename "$(dirname "$receipt")")
  test "$name" = "$(${hostPackage}/bin/korri-plugin unit-name '@korri:ssh')"
  if ${hostPackage}/bin/korri-plugin unit-name '@korri:ssh/other' >/dev/null 2>&1; then
    echo 'unit-name accepted an invalid plugin ID' >&2
    exit 1
  fi
  test "$(readlink "files/nix/var/nix/gcroots/korri-plugin-host/$name/active")" = '${sshPackage}'
  touch "$out"
''

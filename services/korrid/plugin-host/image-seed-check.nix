{
  pkgs,
  hostPackage,
  sshPackage,
  mkPlugin,
}:
let
  fixtures = import ./dependency-fixtures.nix { inherit pkgs mkPlugin; };
  publishers = {
    "@korri" = {
      publicKey = "image-fixture:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
      cacheUrl = "https://cache.example.invalid";
    };
    "@runtime" = {
      publicKey = "runtime-fixture:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
      cacheUrl = "https://runtime.example.invalid";
    };
    "@games" = {
      publicKey = "games-fixture:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
      cacheUrl = "https://games.example.invalid";
    };
  };
  rejectedImages = {
    missing = {
      pluginPackages = [
        sshPackage
        fixtures.transitive
        fixtures.pack
      ];
      error = "missing exact selected dependency";
    };
    wrongVersion = {
      pluginPackages = [
        sshPackage
        fixtures.pack
        fixtures.secondRunner
      ];
      error = "missing exact selected dependency";
    };
    conflict = {
      pluginPackages = [
        sshPackage
        fixtures.conflict
        fixtures.runner
        fixtures.secondRunner
      ];
      error = "conflicting exact versions";
    };
    cycle = {
      pluginPackages = [
        sshPackage
        fixtures.cycle
      ];
      error = "cycle";
    };
    unknown = {
      pluginPackages = [
        sshPackage
        fixtures.pack
        fixtures.runner
      ];
      publishers = builtins.removeAttrs publishers [ "@runtime" ];
      error = "publisher @runtime is not bound";
    };
  };
  image = import ./image-seed.nix {
    inherit pkgs hostPackage publishers;
    pluginPackages = [
      sshPackage
      fixtures.transitive
      fixtures.pack
      fixtures.runner
    ];
  };
in
assert builtins.length image.storePaths == 4;
pkgs.runCommand "korri-plugin-image-seed-check"
  {
    nativeBuildInputs = [
      pkgs.coreutils
      pkgs.jq
    ];
  }
  ''
    mkdir -p files
    ${image.populateRootCommands}
    test "$(find files/var/lib/korri-plugin-host -name selection.json -type f | wc -l)" = 4
    receipt="files/var/lib/korri-plugin-host/$(${hostPackage}/bin/korri-plugin unit-name '@korri:ssh')/selection.json"
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
    ${hostPackage}/bin/korri-plugin seed ${sshPackage} https://cache.example.invalid > leaf.json
    jq -S . leaf.json > leaf-sorted.json
    jq -S . "$receipt" > graph-leaf.json
    cmp leaf-sorted.json graph-leaf.json
    ${pkgs.lib.concatStringsSep "\n" (
      pkgs.lib.mapAttrsToList
        (name: expected: ''
          name=$(${hostPackage}/bin/korri-plugin unit-name '${expected.id}')
          receipt="files/var/lib/korri-plugin-host/$name/selection.json"
          jq -e --arg package '${expected.package}' --arg cache '${expected.cache}' \
            '.package == $package and .provenance.cache_url == $cache and .desired.state == "Enabled" and .previous == null' "$receipt"
          test "$(readlink "files/nix/var/nix/gcroots/korri-plugin-host/$name/active")" = '${expected.package}'
          test "$(stat -c %a "$receipt")" = 600
          test "$(stat -c %a "$(dirname "$receipt")")" = 700
        '')
        {
          runner = {
            id = "@runtime:fake08";
            package = fixtures.runner;
            cache = publishers."@runtime".cacheUrl;
          };
          pack = {
            id = "@games:starter-pack";
            package = fixtures.pack;
            cache = publishers."@games".cacheUrl;
          };
          transitive = {
            id = "@games:collection";
            package = fixtures.transitive;
            cache = publishers."@games".cacheUrl;
          };
        }
    )}
    # Validate the complete graph before even an unrelated leaf writes.
    ${pkgs.lib.concatStringsSep "\n" (
      pkgs.lib.mapAttrsToList (name: rejected: ''
        mkdir '${name}'
        if (
          cd '${name}'
          ${(import ./image-seed.nix {
            inherit pkgs hostPackage;
            publishers = rejected.publishers or publishers;
            inherit (rejected) pluginPackages;
          }).populateRootCommands
          }
        ) 2>'${name}-error'; then
          echo 'image seed accepted ${name}' >&2
          exit 1
        fi
        grep -q '${rejected.error}' '${name}-error'
        test ! -e '${name}/files/var/lib/korri-plugin-host'
      '') rejectedImages
    )}
    # An existing parent receipt must refuse before the runner is written.
    name=$(${hostPackage}/bin/korri-plugin unit-name '@games:starter-pack')
    mkdir -p "collision/files/var/lib/korri-plugin-host/$name"
    printf 'retained\n' > "collision/files/var/lib/korri-plugin-host/$name/selection.json"
    if (cd collision; ${image.populateRootCommands}) 2>collision-error; then
      echo 'image seed overwrote an existing receipt' >&2
      exit 1
    fi
    grep -q 'duplicate seeded plugin' collision-error
    test "$(find collision/files/var/lib/korri-plugin-host -name selection.json | wc -l)" = 1
    test "$(cat "collision/files/var/lib/korri-plugin-host/$name/selection.json")" = retained
    # Collection requires pack and runner, so its blocked parents occur after
    # earlier selections in dependency order. Refuse before any image writes.
    name=$(${hostPackage}/bin/korri-plugin unit-name '@games:collection')
    for parent in "var/lib/korri-plugin-host/$name" "nix/var/nix/gcroots/korri-plugin-host/$name" \
      var/lib nix/var/nix/gcroots; do
      for kind in file symlink; do
        case="parent-$kind-$(printf '%s' "$parent" | tr / -)"
        blocked="$case/files/$parent"
        mkdir -p "$(dirname "$blocked")" "$case/outside"
        printf 'retained\n' > "$case/outside/sentinel"
        if [[ "$kind" = file ]]; then
          printf 'retained\n' > "$blocked"
        else
          ln -s "$PWD/$case/outside" "$blocked"
        fi
        find "$case" -printf '%P %y %m %l\n' | sort > "$case-before"
        if (cd "$case"; ${image.populateRootCommands}) 2>"$case-error"; then
          echo "image seed accepted a $kind destination parent: $parent" >&2
          exit 1
        fi
        grep -q 'incompatible image seed directory' "$case-error"
        find "$case" -printf '%P %y %m %l\n' | sort > "$case-after"
        cmp "$case-before" "$case-after"
        test "$(cat "$case/outside/sentinel")" = retained
        if [[ "$kind" = file ]]; then
          test "$(cat "$blocked")" = retained
        else
          test "$(readlink "$blocked")" = "$PWD/$case/outside"
        fi
      done
    done
    touch "$out"
  ''

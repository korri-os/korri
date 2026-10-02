# The builder must refuse a file that names a store output rather than a path
# inside one. It also owns the source inventory: authors supply a directory,
# never a handwritten manifest list or a single-file compatibility input.
{ pkgs, mkPlugin }:
let
  publisher.namespace = "@korri";
  source = pkgs.runCommand "plugin-source" { } ''
    mkdir -p "$out/src" "$out/node_modules/@fixture/title"
    cat > "$out/plugin.ts" <<'EOF'
    import { prefix } from "./src/prefix.ts";
    import { title } from "@fixture/title";
    export const name = "check";
    export const description = prefix + title;
    EOF
    echo 'export const prefix = "Builder ";' > "$out/src/prefix.ts"
    echo 'export const title = "graph";' > "$out/node_modules/@fixture/title/index.ts"
    echo '{"type":"module","exports":"./index.ts"}' > "$out/node_modules/@fixture/title/package.json"
    echo 'not admitted' > "$out/README.md"
  '';
  bareFile = pkgs.writeText "bare-evidence" "{}";
  bare = mkPlugin {
    inherit publisher source;
    plugin = _: {
      files.evidence = bareFile;
    };
  };
  dependencies = import ./dependency-fixtures.nix { inherit pkgs mkPlugin; };
  invalidRequirement =
    requires:
    !(builtins.tryEval (
      builtins.seq
        (mkPlugin {
          inherit publisher source;
          plugin = _: { inherit requires; };
        }).outPath
        true
    )).success;
  # A builder assertion is an evaluation failure, which tryEval can observe.
  bareRefused = !(builtins.tryEval (builtins.seq bare.outPath true)).success;
  nested = mkPlugin {
    inherit publisher source;
    plugin = _: {
      files.evidence = "${pkgs.writeTextDir "settings.json" "{}"}/settings.json";
    };
  };
in
assert pkgs.lib.assertMsg bareRefused "builder accepted a bare store output as a file";
assert invalidRequirement [ "nixpkgs#fake08" ];
assert invalidRequirement [ "${dependencies.runner}/plugin.ts" ];
assert invalidRequirement [ dependencies.runner.drvPath ];
assert invalidRequirement [
  dependencies.runner
  dependencies.runner
];
assert invalidRequirement (pkgs.lib.replicate 129 dependencies.runner);
pkgs.runCommand "korri-plugin-builder-check"
  {
    nativeBuildInputs = [
      pkgs.jq
      pkgs.python3
    ];
  }
  ''
    test -f ${nested}/manifest.json
    test -f ${nested}/plugin.ts
    test -f ${nested}/src/prefix.ts
    test -f ${nested}/node_modules/@fixture/title/index.ts
    test -f ${nested}/node_modules/@fixture/title/package.json
    test ! -e ${nested}/README.md
    test "$(jq -r .entry ${nested}/manifest.json)" = plugin.ts
    test "$(jq -c .sources ${nested}/manifest.json)" = '["node_modules/@fixture/title/index.ts","node_modules/@fixture/title/package.json","plugin.ts","src/prefix.ts"]'
    grep -q '/settings.json' ${nested}/manifest.json
    # Leaf metadata stays byte-for-byte unchanged: no empty requires field.
    jq -e 'has("requires") | not' ${nested}/manifest.json
    jq -e --arg runner ${dependencies.runner} '.requires == [$runner] and .files == {} and .packages == {}' ${dependencies.pack}/manifest.json
    jq -e '.publisher.namespace == "@runtime" and .sources == ["launch.ts", "plugin.ts"] and (.files.fake08 | endswith("/bin/fixture-fake08"))' ${dependencies.runner}/manifest.json
    test ! -f ${dependencies.pack}/launch.ts

    printf '{}\n' > manifest-base.json
    mkdir path-budget
    printf 'export const name = "path-budget";\n' > path-budget/plugin.ts
    python3 - <<'PY'
    from pathlib import Path

    root = Path("path-budget")
    for index in range(330):
        name = f"{index:03d}-" + ("x" * 196) + ".ts"
        (root / name).write_text("export {};\n")
    PY
    if python3 ${./source-package.py} path-budget path-budget-output manifest-base.json \
      2>path-budget-error; then
      echo "source packager accepted an aggregate relative-path budget overflow" >&2
      exit 1
    fi
    grep -q 'exceeds 64 KiB of relative paths' path-budget-error

    mkdir -p component-budget/a/b/c/d/e/f/g/h
    printf 'export const name = "component-budget";\n' > component-budget/plugin.ts
    python3 - <<'PY'
    from pathlib import Path

    root = Path("component-budget/a/b/c/d/e/f/g/h")
    for index in range(511):
        (root / f"{index:03d}.ts").write_text("export {};\n")
    PY
    if python3 ${./source-package.py} component-budget component-budget-output manifest-base.json \
      2>component-budget-error; then
      echo "source packager accepted an aggregate relative-component budget overflow" >&2
      exit 1
    fi
    grep -q 'exceeds 4096 relative path components' component-budget-error

    mkdir -p "$out"
    ${pkgs.lib.concatStringsSep "\n" (
      pkgs.lib.mapAttrsToList (name: path: "ln -s ${path} \"$out/${name}\"") dependencies
    )}
  ''

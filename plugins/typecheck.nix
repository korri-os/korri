{ pkgs }:
let
  source = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [
      (pkgs.lib.fileset.fileFilter (file: file.hasExt "ts") ./.)
      ../contracts/generated/korrid.ts
      (pkgs.lib.fileset.fileFilter (file: file.hasExt "ts") ../services/korrid/tests/fixtures/first-party)
      ../services/korrid/tests/fixtures/first-party/SHA256SUMS
    ];
  };
  # Match the publisher's libretro-typecheck.nix and zquest-check.nix settings.
  tsconfig = pkgs.writeText "korri-plugins-tsconfig.json" (
    builtins.toJSON {
      compilerOptions = {
        target = "ES2022";
        module = "ESNext";
        moduleResolution = "Bundler";
        strict = true;
        noEmit = true;
        skipLibCheck = true;
      };
      include = [ "plugins/*/plugin.ts" ];
    }
  );
in
pkgs.runCommand "korri-plugins-typecheck" { nativeBuildInputs = [ pkgs.typescript ]; } ''
  cp -R --no-preserve=mode ${source}/. work
  # Preserve producer bytes and restore their original import layout. These
  # are pinned test declarations, never packaged as Core production plugins.
  (cd work/services/korrid/tests/fixtures/first-party && sha256sum -c SHA256SUMS)
  for name in steam fex proton; do
    mkdir -p "work/plugins/$name"
    cp "work/services/korrid/tests/fixtures/first-party/$name/plugin.ts" "work/plugins/$name/plugin.ts"
  done
  cp ${tsconfig} work/tsconfig.json
  cd work
  tsc --project tsconfig.json
  touch "$out"
''

{ pkgs }:
let
  source = pkgs.lib.fileset.toSource {
    root = ../.;
    fileset = pkgs.lib.fileset.unions [
      (pkgs.lib.fileset.fileFilter (file: file.hasExt "ts") ./.)
      ../contracts/generated/korrid.ts
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
  cp ${tsconfig} work/tsconfig.json
  cd work
  tsc --project tsconfig.json
  touch "$out"
''

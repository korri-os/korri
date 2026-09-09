{ pkgs }:
let
  lib = pkgs.lib;
  fs = lib.fileset;
  root = ../..;

  dependencySource = fs.toSource {
    inherit root;
    fileset = fs.unions [
      ./package.json
      ./bun.lock
      ./bunfig.toml
      ../../surfaces/shift/package.json
      ../../surfaces/shift/bun.lock
      ../../surfaces/shift/bunfig.toml
      ../../packages/intrinsic-design/package.json
    ];
  };

  # Keep the repository-relative surface and CSS imports, without copying
  # checkout node_modules, dist, or an incidental npm lock into the build.
  buildSource = fs.toSource {
    inherit root;
    fileset = fs.unions [
      ./package.json
      ./index.html
      ./public
      ./vite.config.ts
      ./tsconfig.json
      ./src
      ../../contracts
      ../../surfaces/shift/src
      ../../surfaces/pico/src
      ../../packages/intrinsic-design/intrinsic.css
      ../../packages/intrinsic-design/recipe.css
    ];
  };

  bunDeps = pkgs.stdenvNoCC.mkDerivation {
    name = "korri-portal-bun-deps";
    src = dependencySource;
    nativeBuildInputs = [
      pkgs.bun
      pkgs.cacert
    ];
    dontConfigure = true;
    buildPhase = ''
      runHook preBuild
      export HOME="$TMPDIR"
      bun install --cwd surfaces/shift --frozen-lockfile --ignore-scripts
      bun install --cwd clients/portal --frozen-lockfile --ignore-scripts
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      # Bun links this local package into /build. Surfaces instead import its
      # CSS by repository-relative path, so the dependency output needs no link.
      rm -rf surfaces/shift/node_modules/@korri/intrinsic-design
      mkdir -p "$out/clients/portal" "$out/surfaces/shift"
      cp -R clients/portal/node_modules "$out/clients/portal/"
      cp -R surfaces/shift/node_modules "$out/surfaces/shift/"
      runHook postInstall
    '';
    outputHashAlgo = "sha256";
    outputHashMode = "recursive";
    outputHash = "sha256-1iwn0h0//EwYcM03p+y9UxS77LCXRkfGHJoLIaeqNnY=";
  };
in
# Bun's dependency tree includes platform-specific build tools. Build this
# data-only output on x86_64-linux; the static assets run on any device.
assert lib.assertMsg (
  pkgs.stdenv.hostPlatform.system == "x86_64-linux"
) "korri-portal: the Bun dependency hash is verified only for x86_64-linux";
pkgs.stdenvNoCC.mkDerivation {
  pname = "korri-portal";
  version = "0.0.0";
  src = buildSource;
  nativeBuildInputs = [
    pkgs.bun
    pkgs.nodejs
  ];
  dontConfigure = true;
  buildPhase = ''
    runHook preBuild
    export HOME="$TMPDIR"
    cp -R ${bunDeps}/clients/portal/node_modules clients/portal/
    cp -R ${bunDeps}/surfaces/shift/node_modules surfaces/shift/
    chmod -R u+w clients/portal/node_modules surfaces/shift/node_modules
    patchShebangs clients/portal/node_modules surfaces/shift/node_modules
    cd clients/portal
    bun run build
    runHook postBuild
  '';
  installPhase = ''
    runHook preInstall
    mkdir -p "$out"
    cp -R dist/. "$out/"
    runHook postInstall
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    runHook preInstallCheck
    cmp public/kiosk-blank.html "$out/kiosk-blank.html"
    test ! -e "$out/runtime.json"
    runHook postInstallCheck
  '';
  meta.platforms = [ "x86_64-linux" ];
}

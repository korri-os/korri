{ pkgs }:
let
  inherit (pkgs) lib;
  root = ../../..;
  source = lib.fileset.toSource {
    inherit root;
    fileset = lib.fileset.unions [
      ../index.html
      ../package.json
      ../public
      ../tsconfig.json
      ../vite.config.ts
      ../src
      ../../../surfaces/shift/package.json
      ../../../surfaces/shift/tsconfig.json
      ../../../surfaces/shift/src
      ../../../surfaces/pico/package.json
      ../../../surfaces/pico/tsconfig.json
      ../../../surfaces/pico/src
      ../../../contracts
      ../../../packages/intrinsic-design/package.json
      ../../../packages/intrinsic-design/intrinsic.css
      ../../../packages/intrinsic-design/recipe.css
    ];
  };

  # Follow the existing portal build's two installs. Pico is compiled from
  # source; its Caliper-only development dependencies do not enter this build.
  dependencies = pkgs.stdenvNoCC.mkDerivation {
    pname = "korri-portal-dependencies";
    version = "0.0.0";
    src = lib.fileset.toSource {
      inherit root;
      fileset = lib.fileset.unions [
        ../package.json
        ../bun.lock
        ../../../surfaces/shift/package.json
        ../../../surfaces/shift/bun.lock
        ../../../packages/intrinsic-design/package.json
      ];
    };
    nativeBuildInputs = [ pkgs.bun ];
    dontConfigure = true;
    dontFixup = true;
    SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
    buildPhase = ''
      runHook preBuild
      export HOME="$TMPDIR/home"
      export BUN_INSTALL_CACHE_DIR="$TMPDIR/bun-cache"
      mkdir -p "$HOME"
      for project in surfaces/shift clients/portal; do
        (
          cd "$project"
          # Keep both builders' native packages under the same dependency hash.
          bun install --frozen-lockfile --ignore-scripts --os=linux --cpu='*'
        )
      done
      # Bun's local-file install must resolve to the build's source, not to a
      # temporary dependency-fetch directory or a stale copy in this output.
      rm -rf surfaces/shift/node_modules/@korri/intrinsic-design
      ln -s ../../../../packages/intrinsic-design surfaces/shift/node_modules/@korri/intrinsic-design
      runHook postBuild
    '';
    installPhase = ''
      runHook preInstall
      for project in surfaces/shift clients/portal; do
        mkdir -p "$out/$project"
        cp -R "$project/node_modules" "$out/$project/"
      done
      runHook postInstall
    '';
    outputHashMode = "recursive";
    outputHashAlgo = "sha256";
    outputHash = "sha256-lLSFbrmwTZJ2RYM0wNlpjVDq6XWmw7Tsp0l8qDhgSeI=";
  };
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "korri-portal";
  version = "0.0.0";
  src = source;
  nativeBuildInputs = [
    pkgs.bun
    pkgs.nodejs
  ];
  dontConfigure = true;
  # Static web assets need no ELF patching or executable shebang rewriting.
  dontFixup = true;
  allowedReferences = [ ];

  buildPhase = ''
    runHook preBuild
    export HOME="$TMPDIR/home"
    mkdir -p "$HOME"
    for project in surfaces/shift clients/portal; do
      cp -R "${dependencies}/$project/node_modules" "$project/"
      chmod -R u+w "$project/node_modules"
    done
    # Only the Nix build uses the reproducible epoch. Ordinary Vite dev/build
    # keeps its wall-clock stamp; no checked-in product source is changed.
    substituteInPlace clients/portal/vite.config.ts \
      --replace-fail 'new Date()' 'new Date(Number(process.env.SOURCE_DATE_EPOCH) * 1000)'
    cd clients/portal
    patchShebangs node_modules/vite/bin/vite.js
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
    test -s "$out/index.html"
    test -d "$out/assets"
    test ! -e "$out/node_modules"
    grep -Fq './assets/' "$out/index.html"
    # Brand assets from clients/portal/public must ship with the bundle.
    test -s "$out/manifest.webmanifest"
    test -s "$out/favicon.svg"
    test -s "$out/icon-512.png"
    js=("$out"/assets/*.js)
    css=("$out"/assets/*.css)
    test -s "''${js[0]}"
    test -s "''${css[0]}"
    # Both source-built surfaces must ship, not just an empty Vite shell.
    grep -Fq 'pico-screen' "''${css[@]}"
    grep -Fq 'data-shift-surface' "''${css[@]}"
    runHook postInstallCheck
  '';

  passthru = { inherit dependencies; };
  meta = {
    description = "Immutable static Korri portal with Pico and Shift";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}

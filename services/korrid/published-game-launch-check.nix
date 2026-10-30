# Test current route/callback dispatch against the unchanged publisher output.
# Only Core and the test harness can build; the plugin is an exact path input.
{ pkgs, korridPackage }:
let
  published = (import ../../nix/product/catalog-plugins.nix {
    inherit pkgs; system = pkgs.stdenv.hostPlatform.system;
  }).plugins;
in
korridPackage.overrideAttrs (old: {
  pname = "korrid-published-game-launch-check";
  # The runtime already built and tested the unchanged Core library. Only
  # compile the acceptance harness here, not another runtime package.
  cargoArtifacts = korridPackage.buildArtifacts;
  outputs = [ "out" ];
  doNotPostBuildInstallCargoBinaries = true;
  buildPhase = ''
    runHook preBuild
    runHook postBuild
  '';
  installPhase = ''
    runHook preInstall
    touch "$out"
    runHook postInstall
  '';
  postInstall = "";
  KORRI_PUBLISHED_MGBA = published.korri-plugin-mgba;
  checkPhase = ''
    runHook preCheck
    cargo test --release --locked --offline --test published_game_launch \
      -- --ignored --exact published_mgba_route_and_launch_prepare
    runHook postCheck
  '';
  # These reviewed catalog fixtures are test inputs, not runtime package data.
  postPatch = (old.postPatch or "") + ''
    mkdir -p ../../docs/research
    cp -R ${../../docs/research/retroarch-plugin-route} ../../docs/research/retroarch-plugin-route
    cp -R ${../../docs/research/android-app-plugin-schema-checkpoint} ../../docs/research/android-app-plugin-schema-checkpoint
  '';
})

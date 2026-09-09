{ pkgs, crane }:
let
  craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
  clean = craneLib.cleanCargoSource ./.;
  vendor = craneLib.vendorCargoDeps { src = clean; };
  common = {
    pname = "korri-plugin-host";
    version = "0.0.0";
    strictDeps = true;
    cargoVendorDir = vendor;
  };
  artifacts = craneLib.buildDepsOnly (common // { src = clean; });
  # The separate CLI crate uses korrid's actual evaluator. Materialize exactly
  # that shared source and its existing example test, rather than cloning it.
  source = pkgs.runCommand "korri-plugin-host-source" { } ''
    mkdir -p "$out/src" "$out/examples"
    cp -R ${clean} "$out/plugin-host"
    cp ${../src/script.rs} "$out/src/script.rs"
    cp ${../examples/catalog.plugin.ts} "$out/examples/catalog.plugin.ts"
  '';
in
craneLib.buildPackage (
  common
  // {
    src = source;
    cargoArtifacts = artifacts;
    postUnpack = ''sourceRoot="$sourceRoot/plugin-host"'';
    nativeBuildInputs = [ pkgs.makeWrapper ];
    KORRI_TEST_CURL = "${pkgs.curl}/bin/curl";
    postInstall = ''
      wrapProgram "$out/bin/korri-plugin" \
        --set KORRI_PLUGIN_NIX ${pkgs.nix}/bin/nix \
        --set KORRI_PLUGIN_SYSTEMCTL ${pkgs.systemd}/bin/systemctl \
        --set KORRI_PLUGIN_CURL ${pkgs.curl}/bin/curl

      # korri-publish is build-side only; it does not need systemctl.
      # The Nix binary is injected so publication does not require an ambient
      # nix on PATH, matching the same isolation used by korri-plugin.
      wrapProgram "$out/bin/korri-publish" \
        --set KORRI_PUBLISH_NIX ${pkgs.nix}/bin/nix
    '';
    meta.mainProgram = "korri-plugin";
  }
)

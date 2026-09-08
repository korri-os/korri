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
    postInstall = ''
      wrapProgram "$out/bin/korri-plugin" \
        --set KORRI_PLUGIN_NIX ${pkgs.nix}/bin/nix \
        --set KORRI_PLUGIN_SYSTEMCTL ${pkgs.systemd}/bin/systemctl
    '';
    meta.mainProgram = "korri-plugin";
  }
)

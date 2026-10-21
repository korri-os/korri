{ pkgs, crane }:
let
  craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
  commonArgs = {
    src = craneLib.cleanCargoSource ./.;
    pname = "korri-limitsd";
    version = "0.0.0";
    strictDeps = true;
  };
in
craneLib.buildPackage (
  commonArgs
  // {
    cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    meta.mainProgram = "korri-limitsd";
  }
)

{ pkgs, crane }:
let
  craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.default;
  commonArgs = {
    src = craneLib.cleanCargoSource ./.;
    pname = "korri-kiosk";
    version = "0.0.0";
    strictDeps = true;
    meta.mainProgram = "korri-kiosk";
  };
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;
in
craneLib.buildPackage (commonArgs // { inherit cargoArtifacts; })

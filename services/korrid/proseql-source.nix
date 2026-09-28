{ pkgs, proseql }:

let
  cachePath = ".cache/proseql";
  crateRoot = "services/korrid";
  linkedPath = "$KORRI_ROOT/${crateRoot}/${cachePath}";
in
{
  inherit cachePath;

  enginePath = "${cachePath}/crates/proseql-engine";
  formatsPath = "${cachePath}/crates/proseql-formats";
  storagePath = "${cachePath}/crates/proseql-storage";

  hydrateShell = ''
    mkdir -p "$KORRI_ROOT/${crateRoot}/.cache"
    ln -sfn "${proseql}" "${linkedPath}"
  '';

  dummySourceScript = ''
    mkdir -p "$out/${crateRoot}/.cache"
    ln -s "${proseql}" "$out/${crateRoot}/${cachePath}"
  '';

  composeCargoSource =
    src:
    pkgs.runCommand "korrid-source-with-proseql" { } ''
      mkdir -p "$out"
      cp -R --no-preserve=mode,ownership ${src}/. "$out/"
      mkdir -p "$out/${crateRoot}/.cache"
      ln -s "${proseql}" "$out/${crateRoot}/${cachePath}"
    '';
}

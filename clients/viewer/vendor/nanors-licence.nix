# nanors' licence (MIT, Joseph Calderon). nightjar-core's
# `nightjar/protocol/client/src/fec.rs` is "Ported from nanors (MIT, Joseph
# Calderon)", so the notices of every artefact that carries nightjar-core
# carry this text (vendor/README.md): the PC package's (../notices.nix) and
# the Korri app's (../../android-app/notices.nix). It is the file as nanors'
# commit 0b09d1ee (2021-01-25) created it; no later commit changed it.
{ pkgs }:
pkgs.fetchurl {
  name = "nanors-0b09d1ee-LICENSE";
  url = "https://raw.githubusercontent.com/sleepybishop/nanors/0b09d1ee65b70fe3862224f710331b46e350cad7/LICENSE";
  hash = "sha256-P92l8BHYSQMxlQOY6GQn1n364F4EhoFHbCxrjDS90DM=";
}

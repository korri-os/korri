# nanors' licence (MIT, Joseph Calderon). nightjar-core's
# `nightjar/protocol/client/src/fec.rs` is "Ported from nanors (MIT, Joseph
# Calderon)", so every artefact that carries nightjar-core carries this text
# (vendor/README.md): the PC package and its corresponding source
# (../default.nix) and the Korri app's notices
# (../../android-app/notices.nix). It is the file at the nanors commit that
# last changed it, 0b09d1ee (2021-01-25), the repository's first.
{ pkgs }:
pkgs.fetchurl {
  name = "nanors-0b09d1ee-LICENSE";
  url = "https://raw.githubusercontent.com/sleepybishop/nanors/0b09d1ee65b70fe3862224f710331b46e350cad7/LICENSE";
  hash = "sha256-P92l8BHYSQMxlQOY6GQn1n364F4EhoFHbCxrjDS90DM=";
}

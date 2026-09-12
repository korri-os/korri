# Korri's cache: where it is, and who may sign for it.
#
# Several consumers need these facts and must never disagree.
# nix/base/default.nix puts the metadata URL and the signing keys into every
# device's Nix configuration. The publisher asks which substituter is its own so
# it can treat the rest as upstream caches to filter against; one that carried
# its own copy of the address could filter against a cache the devices do not
# trust, and the closure would then break at install instead of at publish. The
# repositories that configure Korri's builders read the same two lists, so
# revoking a machine stays one edit here.
#
# The flake exposes this as `cache`, which keeps it readable by `nix eval` and by
# another flake without evaluating a device configuration.
let
  repo = "korri-os/nix-cache";
  metadataTag = "cache";
  # NAR payloads live on dated batch releases under the same download root, so
  # the publisher appends its batch tag rather than composing a second URL.
  downloadBase = "https://github.com/${repo}/releases/download/";
in
{
  inherit repo metadataTag downloadBase;
  url = "${downloadBase}${metadataTag}/";
  # One key per builder, so a machine can be revoked by deleting its line
  # without reissuing the others. require-sigs stays true everywhere, so an
  # unsigned path fails the download rather than being built on a device.
  publicKeys = [
    "korri-cache-fuji-1:E8MOww6FoNRlVavEll8JPc2XHYC4HhZnrhqQcd64OtQ="
    "korri-cache-zao-1:thKjQnMnPl8AqTuZWJTn+ej9BORTPqzleGue4ZfJ2u4="
    # CI's key is the weakest of the three: it lives in GitHub Actions secrets,
    # so anyone who can land a workflow change can sign with it. It is here
    # because a device that cannot install what CI built gains nothing from CI.
    "korri-cache-ci-1:iH8gsPMtGrreeuXt2kt6M2ca+y4u/dO+T2km6AUuI8c="
  ];
}

# Korri's cache address, written once.
#
# Two consumers need it and must never disagree. nix/base/default.nix puts the
# metadata URL in every device's substituter list, and the publisher asks which
# entry of that list is its own so it can treat the rest as upstream caches to
# filter against. A publisher that carried its own copy of the address could
# filter against a cache the devices do not trust, and the closure would then
# break at install instead of at publish.
#
# The flake exposes this as `cache`, so the publisher reads the same values
# through `nix eval` rather than repeating them in shell.
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
}

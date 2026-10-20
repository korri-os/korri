# A source-only acquisition provider with a curated catalog. Its handlers
# make no requests; korrid downloads each resolved file from this host only,
# on every redirect hop.
{ pkgs }:
{
  hosts = [ "hh3.gbdev.io" ];
}

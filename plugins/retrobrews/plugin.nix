# A source-only acquisition provider with a curated catalog. Its handlers
# make no requests; korrid downloads each resolved raw file from this host
# only, on every redirect hop. Claim URLs on github.com are shown, not fetched.
{ pkgs }:
{
  hosts = [ "raw.githubusercontent.com" ];
}

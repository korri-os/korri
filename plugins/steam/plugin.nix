# One session program owns all native paths. plugin.ts names only this file.
{ pkgs }:
let
  steam = import ./packages { inherit pkgs; };
in
{
  packages = {
    steam-session = steam.session;
  };
  files."steam-session" = "${steam.session}/bin/steam-session";
}

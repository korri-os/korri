# One session program owns Steam's native paths. plugin.ts names this file and
# hands the session the FEX and Proton files of the exact required builds.
# The publisher composition builds those plugins and passes their outputs.
{
  pkgs,
  fexPlugin,
  protonPlugin,
}:
let
  steam = import ./packages { inherit pkgs; };
in
{
  requires = [
    fexPlugin
    protonPlugin
  ];
  packages = {
    steam-session = steam.session;
  };
  files."steam-session" = "${steam.session}/bin/steam-session";
}

# A whole emulator in its own closure. Nothing here references another plugin:
# no frontend package, no family package, no shared core directory.
{ pkgs }:
{
  packages = {
    ppsspp = pkgs.ppsspp;
  };
  files = {
    # The runner's `program` key names this file. Point at the binary exactly
    # rather than deriving it from the package name.
    ppsspp = "${pkgs.ppsspp}/bin/ppsspp";
  };
}

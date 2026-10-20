# Mega Man Rock N Roll. plugin.ts declares the game, its ZIP artifact and the
# FEX runner, and names the two programs below. The ZIP is not in the closure:
# korrid downloads the file plugin.ts pins into its cache and runs
# game-unpack on it. The runner gets the FEX config and rootfs of the exact
# required @korri:fex build; the publisher composition builds that plugin and
# passes its output.
{
  pkgs,
  fexPlugin,
}:
let
  unpack = pkgs.writeShellApplication {
    name = "mega-man-rock-n-roll-unpack";
    runtimeInputs = [ pkgs.python3 ];
    text = ''exec python3 ${./game/unpack.py} ${./game/contents} "$@"'';
  };
  launch = pkgs.writeShellApplication {
    name = "mega-man-rock-n-roll-fex";
    runtimeInputs = [ pkgs.fex ];
    text = builtins.readFile ./runner/fex-launch.sh;
  };
in
{
  requires = [ fexPlugin ];
  packages = {
    mega-man-rock-n-roll-unpack = unpack;
    mega-man-rock-n-roll-fex = launch;
  };
  files = {
    game-unpack = "${unpack}/bin/mega-man-rock-n-roll-unpack";
    fex-launch = "${launch}/bin/mega-man-rock-n-roll-fex";
  };
}

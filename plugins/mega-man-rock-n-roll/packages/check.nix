# Focused source checks. Invoke with build-host pkgs; the tests unpack small
# generated ZIPs and run the launcher with a recording FEX. Nothing
# here downloads the game, and nothing runs on a device.
{ pkgs }:
pkgs.runCommand "mega-man-rock-n-roll-check"
  {
    nativeBuildInputs = with pkgs; [
      bash
      coreutils
      python3
      shellcheck
    ];
  }
  ''
    mkdir plugin
    cp -r ${../game} plugin/game
    cp -r ${../runner} plugin/runner
    chmod -R u+w plugin
    (cd plugin && python3 -m unittest -v game/test_unpack.py runner/test_launch.py)
    bash -n plugin/runner/fex-launch.sh
    shellcheck -s bash plugin/runner/fex-launch.sh
    touch "$out"
  ''

# Host-only native parser checks; never opens a sound card.
{ pkgs }:
let
  ucm = pkgs.callPackage ./ucm.nix { };
in
pkgs.runCommand "r36tmax-speaker-ucm-check" { nativeBuildInputs = [ pkgs.python3 ]; } ''
  python3 ${./test-ucm.py} ${ucm}/share/alsa/ucm2 ${pkgs.alsa-lib}/lib/libasound.so.2
  # The native tree remains complete; only the cited HiFi policy may differ.
  diff -r ${pkgs.alsa-ucm-conf}/share/alsa ${ucm}/share/alsa > difference || test "$?" = 1
  test "$(grep -c '^diff ' difference)" = 1
  grep '^diff ' difference | grep -F '/Rockchip/rk817-sound/HiFi.conf'
  touch "$out"
''

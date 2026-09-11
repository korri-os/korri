# Inspect the generated config from the real WirePlumber service environment.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  dataDirs = configuration.config.systemd.user.services.wireplumber.environment.XDG_DATA_DIRS;
  configsShare = lib.head (lib.splitString ":" dataDirs);
in
pkgs.runCommand "rg353m-audio-check" { nativeBuildInputs = [ pkgs.python3 ]; } ''
  python3 ${./audio-check.py} ${configsShare}/wireplumber/wireplumber.conf.d/51-rg353m-default-audio.conf
  touch "$out"
''

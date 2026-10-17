{ pkgs }:
let
  proton = pkgs.callPackage ./proton-cachyos-arm64 { };
  rootfs = import ./fex-rootfs.nix { inherit pkgs; };
  native = import ./steam-korri { inherit pkgs proton settings; };
  settings = pkgs.writeShellApplication {
    name = "steam-vdf-settings";
    runtimeInputs = [ pkgs.python3 ];
    text = ''exec python3 ${../session/vdf-settings.py} "$@"'';
  };
  sessionRun = pkgs.writeShellApplication {
    name = "steam-session-run";
    runtimeInputs = with pkgs; [
      coreutils
      findutils
      gnugrep
      procps
      util-linux
    ];
    text =
      pkgs.lib.replaceStrings [ "@rootfs@" "@fhs@" "@helpers@" "@proton@" "@resources@" "@settings@" ]
        (map toString [
          rootfs
          native.fhs
          native.helpers
          proton
          native.resources
          settings
        ])
        (builtins.readFile ../session/steam-session.sh);
  };
  session = pkgs.writeShellApplication {
    name = "steam-session";
    runtimeInputs = [
      pkgs.dbus
      pkgs.coreutils
    ];
    text = ''
      if [[ "$#" != 1 || ! "$1" =~ ^[1-9][0-9]*$ ]]; then
        echo 'usage: steam-session <appid>' >&2
        exit 2
      fi
      if [[ "''${STEAM_HOME:-}" != /* || "''${HOME:-}" != /* ]]; then
        echo 'steam-session: STEAM_HOME and HOME must be absolute paths' >&2
        exit 2
      fi
      if ! mkdir -p "$STEAM_HOME" || [[ ! -w "$STEAM_HOME" ]]; then
        echo 'steam-session: STEAM_HOME is not a writable directory' >&2
        exit 2
      fi
      status=0
      dbus-run-session -- ${sessionRun}/bin/steam-session-run "$1" || status=$?
      case "$status" in
        0|2|124|126|130|143) exit "$status" ;;
        *) echo "steam-session: Steam failed: session bus or client exited with status $status" >&2; exit 126 ;;
      esac
    '';
  };
in
native
// {
  inherit
    proton
    rootfs
    settings
    session
    sessionRun
    ;
  fex = pkgs.fex;
}

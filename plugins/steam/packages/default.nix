{ pkgs }:
let
  # The single Nix definition of the tool name, owned by @korri:proton.
  protonToolName = (import ../../proton/cachyos/tool-name.nix).toolName;
  native = import ./steam-korri { inherit pkgs settings; };
  settingsSource = pkgs.writeText "vdf-settings.py" (
    pkgs.lib.replaceStrings [ "@protonToolName@" ] [ protonToolName ] (
      builtins.readFile ../session/vdf-settings.py
    )
  );
  settings = pkgs.writeShellApplication {
    name = "steam-vdf-settings";
    runtimeInputs = [ pkgs.python3 ];
    text = ''exec python3 ${settingsSource} "$@"'';
  };
  sessionRun = pkgs.writeShellApplication {
    name = "steam-session-run";
    runtimeInputs = with pkgs; [
      acl
      coreutils
      diffutils
      findutils
      gnugrep
      procps
      util-linux
    ];
    text =
      pkgs.lib.replaceStrings
        [ "@fhs@" "@helpers@" "@settings@" "@protonToolName@" ]
        [
          "${native.fhs}"
          "${native.helpers}"
          "${settings}"
          protonToolName
        ]
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
      # plugin.ts passes these from the exact required @korri:fex and
      # @korri:proton builds: the FEX config file and the artifact
      # directories korrid readied before the launch.
      for name in KORRI_FEX_CONFIG KORRI_FEX_ROOTFS KORRI_PROTON_CACHYOS; do
        if [[ "''${!name:-}" != /* ]]; then
          echo "steam-session: $name must be an absolute path" >&2
          exit 2
        fi
        if [[ ! -d "''${!name}" ]]; then
          echo "steam-session: $name is not a directory" >&2
          exit 2
        fi
      done
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
    settings
    session
    sessionRun
    protonToolName
    ;
  fex = pkgs.fex;
}

# mega-man-rock-n-roll-fex <game-dir>
#
# Run the x86_64 Godot executable MegaMan_RocknRoll from <game-dir> under
# FEX. plugin.ts passes the directory of the release's data.pck, HOME, and the
# exact required @korri:fex build's config directory and ready rootfs:
#
#   HOME              the game's home; Godot keeps saves in ~/.godot
#   KORRI_FEX_CONFIG  files["@korri:fex/config"]
#   KORRI_FEX_ROOTFS  artifacts["@korri:fex/rootfs"]
#
# FEX, the interpreter in FEX 2608 (formerly FEXInterpreter), comes from the
# pkgs.fex that plugin.nix bundles.
# The game is X11 only and uses the game unit's Xwayland DISPLAY. Do not set
# WAYLAND_DISPLAY.
#
# Exit status: 2 if an input is missing or wrong; otherwise the game's own.
set -Eeuo pipefail

fail() { printf 'mega-man-rock-n-roll-fex: %s\n' "$*" >&2; exit 2; }

(( $# == 1 )) || fail 'usage: mega-man-rock-n-roll-fex <game-dir>'
game="$1"
[[ "$game" == /* && -d "$game" ]] || fail "game directory is not an absolute directory: $game"
for file in data.pck MegaMan_RocknRoll; do
  [[ -f "$game/$file" ]] || fail "game directory is missing $file"
done
for name in HOME KORRI_FEX_CONFIG KORRI_FEX_ROOTFS; do
  [[ "${!name:-}" == /* ]] || fail "$name must be an absolute path"
  [[ -d "${!name}" ]] || fail "$name is not a directory"
done

export FEX_ROOTFS="$KORRI_FEX_ROOTFS"
export FEX_APP_CONFIG_LOCATION="$KORRI_FEX_CONFIG/"
# FEX's own default location, fixed so it never depends on XDG_DATA_HOME.
export FEX_APP_DATA_LOCATION="$HOME/.fex-emu/"
# Godot loads data.pck from its working directory.
cd "$game"
exec FEX "$game/MegaMan_RocknRoll"

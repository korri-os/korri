# Check the Sunshine plugin's native root helpers against the store paths they
# actually reference. The receiver's setup runs before any plugin unit, so a
# missing tool or a udev rule that udevd refuses stops the whole plugin host.
{
  pkgs,
  inputdPackage,
  sunshinePackage,
}:
let
  plugin = import ../../../plugins/sunshine/plugin.nix {
    inherit pkgs inputdPackage sunshinePackage;
  };
in
pkgs.runCommand "korri-sunshine-plugin-native-check"
  {
    nativeBuildInputs = [
      pkgs.gnugrep
      pkgs.systemd
    ];
  }
  ''
    set -euo pipefail
    setup=${plugin.files.setup}
    rules=${plugin.files.input-rules}

    # Every store tool the setup helper calls must exist in its closure.
    tools="$(grep -oE '/nix/store/[a-z0-9]{32}-[^/ ]+/bin/[A-Za-z0-9_.-]+' "$setup" | sort -u)"
    test -n "$tools"
    for tool in $tools; do
      if ! test -x "$tool"; then
        echo "setup helper calls a missing tool: $tool" >&2
        exit 1
      fi
    done

    # Every store tool a rule runs must exist too.
    for tool in $(grep -oE '/nix/store/[a-z0-9]{32}-[^/ ]+/bin/[A-Za-z0-9_.-]+' "$rules" | sort -u); do
      if ! test -x "$tool"; then
        echo "udev rule runs a missing tool: $tool" >&2
        exit 1
      fi
    done

    # udevd ignores GROUP= for a group that is not a system group. The seat
    # event nodes belong to the korri login user (GID 1000), which is not one,
    # so a GROUP= assignment on them silently leaves the nodes root-only.
    if grep -E 'Korri Seat' "$rules" | grep -qE 'GROUP='; then
      echo "seat event rule assigns GROUP=, which udevd refuses for GID 1000" >&2
      exit 1
    fi

    udevadm verify --resolve-names=never --no-style "$rules"
    touch "$out"
  ''

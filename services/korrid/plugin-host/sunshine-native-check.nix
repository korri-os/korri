# Check the plugin-owned UHID setup without granting it core seat ownership.
{
  pkgs,
  sunshinePackage,
}:
let
  plugin = import ../../../plugins/sunshine/plugin.nix {
    inherit pkgs sunshinePackage;
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
    sunshine_unit=${plugin.services.korri-sunshine}
    setup_unit=${plugin.services.korri-sunshine-input-setup}

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

    # The plugin must not own or trigger changes to persistent core seats.
    ! grep -E 'Korri Seat|SUBSYSTEM=="input"' "$rules"
    ! grep -F -- '--subsystem-match=input' "$setup"
    ! grep -E 'RuntimeDirectory=korri-input-seat|korri-bundle-launch' "$setup_unit"
    grep -Fx 'Type=exec' "$setup_unit"
    grep -F 'ExecStartPre=+' "$setup_unit"
    grep -F 'ExecStopPost=+' "$setup_unit"
    grep -E '^ExecStart=/nix/store/[^ ]+/bin/sleep infinity$' "$setup_unit"
    ! grep -E '^ExecStart=\+|^RemainAfterExit=' "$setup_unit"
    grep -E '^Requires=.*korri-sunshine-input-setup.service.*korri-input-seat-receiver.service' "$sunshine_unit"
    ! grep -F 'korri-sunshine-input-seat-receiver.service' "$sunshine_unit"
    grep -Fx 'Environment=KORRI_INPUT_SEAT_MIRROR_SOCKET=/run/korri-input-seat/sunshine-input-seat.sock' "$sunshine_unit"
    grep -F 'runtime_dir.join("sunshine-input-seat.sock")' ${../../inputd/src/input_seat_receiver.rs}

    # The host owns /dev/uinput (korri-inputd:uinput 0660). A plugin rule that
    # sets its owner, group or mode takes the node from every other holder,
    # and udev's last matching rule decides which plugin loses.
    if grep -qE 'KERNEL=="uinput"' "$rules"; then
      echo "plugin udev rule changes /dev/uinput, which the host owns" >&2
      exit 1
    fi

    # Only Sunshine opens /dev/uinput; the setup unit owns no devices.
    grep -qE '^SupplementaryGroups=(.* )?uinput( |$)' "$sunshine_unit"

    udevadm verify --resolve-names=never --no-style "$rules"
    touch "$out"
  ''

# Check shared-seat ownership against the actual packaged native artifacts.
# Producer tool-closure and native-unit assertions belong to the publisher.
{
  pkgs,
  sunshinePlugin,
}:
pkgs.runCommand "korri-sunshine-plugin-native-check"
  {
    nativeBuildInputs = [
      pkgs.jq
      pkgs.gnugrep
    ];
  }
  ''
    set -euo pipefail
    manifest=${sunshinePlugin}/manifest.json
    setup="$(jq -er '.files.setup' "$manifest")"
    rules="$(jq -er '.files["input-rules"]' "$manifest")"
    sunshine_unit="$(jq -er '.services["korri-sunshine"]' "$manifest")"
    setup_unit="$(jq -er '.services["korri-sunshine-input-setup"]' "$manifest")"

    # The plugin must not own or trigger changes to persistent core seats.
    ! grep -E 'Korri Seat|SUBSYSTEM=="input"' "$rules"
    ! grep -F -- '--subsystem-match=input' "$setup"
    ! grep -E 'RuntimeDirectory=korri-input-seat|korri-bundle-launch' "$setup_unit"
    grep -E '^Requires=.*korri-sunshine-input-setup.service.*korri-input-seat-receiver.service' "$sunshine_unit"
    ! grep -F 'korri-sunshine-input-seat-receiver.service' "$sunshine_unit"
    grep -Fx 'Environment=KORRI_INPUT_SEAT_MIRROR_SOCKET=/run/korri-input-seat/sunshine-input-seat.sock' "$sunshine_unit"
    grep -F 'runtime_dir.join("sunshine-input-seat.sock")' ${../../inputd/src/input_seat_receiver.rs}

    # The host owns /dev/uinput. A plugin must not take it from other holders.
    if grep -qE 'KERNEL=="uinput"' "$rules"; then
      echo "plugin udev rule changes /dev/uinput, which the host owns" >&2
      exit 1
    fi
    grep -qE '^SupplementaryGroups=(.* )?uinput( |$)' "$sunshine_unit"
    touch "$out"
  ''

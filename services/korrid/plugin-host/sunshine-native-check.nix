# Check shared-seat ownership against the actual packaged native artifacts.
# Producer tool-closure and native-unit assertions belong to the publisher.
{
  pkgs,
  sunshinePlugin,
}:
let
  # Extracted from the shipped sunshine-korri provenance. Core's own nixpkgs
  # has an older Sunshine; using pkgs.sunshine.src would check the wrong input.
  sourceHash = "sha256-HqbswLvX/UiY3nOwxSesBMqnFAF0zKP1ueE6PwDtTNs=";
  source = pkgs.fetchFromGitHub {
    owner = "LizardByte";
    repo = "Sunshine";
    rev = "v2026.914.233613";
    fetchSubmodules = true;
    hash = sourceHash;
  };
in
pkgs.runCommand "korri-sunshine-plugin-native-check"
  {
    nativeBuildInputs = [
      pkgs.jq
      pkgs.gnugrep
      pkgs.python3
    ];
  }
  ''
    set -euo pipefail
    manifest=${sunshinePlugin}/manifest.json
    setup="$(jq -er '.files.setup' "$manifest")"
    rules="$(jq -er '.files["input-rules"]' "$manifest")"
    sunshine_unit="$(jq -er '.services["korri-sunshine"]' "$manifest")"
    setup_unit="$(jq -er '.services["korri-sunshine-input-setup"]' "$manifest")"
    sunshine="$(jq -er '.packages.sunshine' "$manifest")"
    grep -Fx 'approved_base_sunshine_source_hash=${sourceHash}' \
      "$sunshine/share/korri/sunshine-korri/provenance"
    python3 ${./check-sunshine-input.py} ${source} \
      ${pkgs.lib.escapeShellArg (
        builtins.toJSON (import ../../../nix/product/sunshine-input-identifiers.nix)
      )}

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

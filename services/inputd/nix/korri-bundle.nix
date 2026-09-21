{
  pkgs,
  inputdPackage,
  inputplumberKorri,
  korridPackage,
}:

pkgs.runCommand "korri-bundle-0.0.0" { } ''
  set -euo pipefail
  mkdir -p "$out/bin" "$out/share"
  ln -s ${inputplumberKorri}/bin/inputplumber "$out/bin/inputplumber"
  ln -s ${inputdPackage}/bin/korri-inputd "$out/bin/korri-inputd"
  ln -s ${inputdPackage}/bin/korri-input-seat-receiver "$out/bin/korri-input-seat-receiver"
  ln -s ${korridPackage}/bin/korrid "$out/bin/korrid"
  ln -s ${korridPackage}/bin/korri-local-signer "$out/bin/korri-local-signer"
  ln -s ${inputplumberKorri}/share/inputplumber "$out/share/inputplumber"
  ln -s ${inputplumberKorri}/share/inputplumber/profiles/korri-60-xbox_one_gamepad.yaml \
    "$out/share/korri-input-profile"
''

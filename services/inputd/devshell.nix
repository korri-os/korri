{ pkgs }:
let
  rustToolchain = pkgs.rust-bin.stable.latest.default.override {
    targets = [ "wasm32-unknown-unknown" ];
    extensions = [
      "clippy"
      "rustfmt"
    ];
  };
in
pkgs.mkShell {
  packages = [
    rustToolchain
    pkgs.cargo-audit
    pkgs.dbus
    pkgs.nixfmt-rfc-style
    pkgs.shellcheck
  ];
  shellHook = ''
    export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/inputd-target"
    echo "Inputd toolchain ready"
  '';
}

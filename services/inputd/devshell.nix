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
  # Real immutable action fixtures, independent of the build host's OS layout.
  # KORRI_TEST_TRUE is also the plugin-host's existing no-effect helper seam.
  KORRI_TEST_TRUE = "${pkgs.coreutils}/bin/true";
  KORRI_TEST_SHELL = pkgs.runtimeShell;
  # Use the daemon package's native session policy, not /etc on the build host.
  DBUS_TEST_SESSION_CONFIG = "${pkgs.dbus}/share/dbus-1/session.conf";

  shellHook = ''
    export CARGO_TARGET_DIR="$KORRI_ROOT/.cache/inputd-target"
    echo "Inputd toolchain ready"
  '';
}

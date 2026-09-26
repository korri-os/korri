{
  pkgs,
  sway ? import ./sway-activity.nix { inherit pkgs; },
}:
pkgs.runCommand "sway-controller-activity-check"
  {
    nativeBuildInputs = [
      pkgs.stdenv.cc
      pkgs.pkg-config
      pkgs.wayland-scanner
      pkgs.python3
      sway
      pkgs.swayidle
      pkgs.wev
      pkgs.coreutils
      pkgs.dbus
    ];
    buildInputs = [ pkgs.wayland ];
  }
  ''
    protocol=${pkgs.wlr-protocols}/share/wlr-protocols/unstable/wlr-virtual-pointer-unstable-v1.xml
    wayland-scanner client-header "$protocol" virtual-pointer.h
    wayland-scanner private-code "$protocol" virtual-pointer.c
    $CC -Wall -Wextra -Werror -I. ${../tests/sway-test-pointer.c} virtual-pointer.c \
      $(pkg-config --cflags --libs wayland-client) -o sway-test-pointer
    export PATH="$PWD:$PATH"
    export HOME="$TMPDIR"
    dbus-run-session --config-file=${pkgs.dbus}/share/dbus-1/session.conf -- \
      python3 ${../tests/sway_activity.py}
    touch "$out"
  ''

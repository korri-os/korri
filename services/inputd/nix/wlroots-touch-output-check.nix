# Public wlroots backend test in an isolated guest; no host input is passed in.
{ pkgs, wlroots }:
let
  probe = pkgs.stdenv.mkDerivation {
    name = "wlroots-touch-output-probe";
    dontUnpack = true;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [
      wlroots
      pkgs.wayland
      pkgs.libinput
      pkgs.pixman
    ];
    buildPhase = ''
      $CC -Wall -Wextra -Werror ${./wlroots-touch-output-check.c} \
        $(pkg-config --cflags --libs wlroots-0.20 wayland-server libinput pixman-1) -o probe
    '';
    installPhase = ''mkdir -p "$out/bin"; cp probe "$out/bin/wlroots-touch-output-probe"'';
  };
  python = pkgs.python3.withPackages (ps: [ ps.evdev ]);
in
pkgs.testers.runNixOSTest {
  name = "wlroots-touch-output";
  nodes.machine =
    { ... }:
    {
      boot.kernelModules = [ "uinput" ];
      services.seatd.enable = true;
      services.udev.extraRules = ''
        SUBSYSTEM=="input", KERNEL=="event*", ATTRS{phys}=="korri-touch-test/top", ENV{WL_OUTPUT}="DSI-2"
        SUBSYSTEM=="input", KERNEL=="event*", ATTRS{phys}=="korri-touch-test/bottom", ENV{WL_OUTPUT}="DSI-1"
      '';
      virtualisation.memorySize = 1024;
    };
  testScript = ''
    machine.start()
    machine.wait_for_unit("seatd.service")
    machine.succeed("env KORRI_TOUCH_TEST_VM=1 LIBSEAT_BACKEND=seatd ${python}/bin/python3 ${./wlroots-touch-output-check.py} ${probe}/bin/wlroots-touch-output-probe")
  '';
}

{
  runCommand,
  stdenv,
  python3,
  patch,
  retroarchSource,
  readOnlyPatch,
}:
runCommand "retroarch-udev-read-only-check"
  {
    nativeBuildInputs = [
      stdenv.cc
      python3
      patch
    ];
  }
  ''
    mkdir -p input/drivers_joypad
    cp ${retroarchSource}/input/drivers_joypad/udev_joypad.c input/drivers_joypad/
    chmod u+w input/drivers_joypad/udev_joypad.c
    patch --fuzz=0 -p1 < ${readOnlyPatch}
    python3 ${./retroarch-udev-read-only-extract.py} \
      input/drivers_joypad/udev_joypad.c udev-joypad-extracted.h
    # Upstream udev_add_pad has unused parameters and local variables.
    $CC -std=c99 -Wall -Wextra -Werror -Wno-unused-parameter -Wno-unused-variable \
      -I. -I${retroarchSource}/libretro-common/include \
      ${./retroarch-udev-read-only-check.c} -o udev-read-only-check
    ./udev-read-only-check
    touch "$out"
  ''

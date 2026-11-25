# Compile the exact pure policy included by the kernel driver on the build host.
{ pkgs }:
pkgs.runCommand "tabs6-typec-policy-check" { nativeBuildInputs = [ pkgs.stdenv.cc ]; } ''
  cc -std=c11 -Wall -Wextra -Werror -pedantic \
    ${./drivers/s2mm005-policy.test.c} -I${./drivers} -o policy-test
  ./policy-test ${../usb-role/phase-a-transition.txt} ${../usb-role/sink-host-charging.txt} \
    ${../usb-role/phase-b-flap.txt} ${../usb-role/phase-b-host-transition.txt} \
    ${../usb-role/sink-host-full-flap.txt} \
    ${../usb-role/sink-host-full-discharging-dips.txt} \
    ${../usb-role/kishi-unpowered-cycle-replug.txt} \
    ${../usb-role/kishi-battery-unplug-devt3.txt}
  touch "$out"
''

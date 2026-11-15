# Inspect the actual merged DTB and replacement module shipped to haku.
{ pkgs, configuration }:
let
  inherit (pkgs) lib;
  inherit (configuration) config;
  kernel = config.boot.kernelPackages.kernel;
  selected = lib.findFirst (
    p: lib.getName p == "adc-joystick-rg353m"
  ) null config.boot.extraModulePackages;
  dtb = "${config.hardware.deviceTree.package}/${config.hardware.deviceTree.name}";
  module = "${selected}/lib/modules/${kernel.modDirVersion}/updates/adc-joystick.ko";
in
assert lib.assertMsg (
  selected != null
) "RG353M calibration needs the corrected ADC joystick module";
pkgs.runCommand "rg353m-input-check"
  {
    nativeBuildInputs = [
      pkgs.dtc
      pkgs.kmod
    ];
  }
  ''
    test "$(fdtget -t i ${dtb} /adc-joystick poll-interval)" = 10
    for spec in '0 0 960 170' '1 3 90 930' '2 1 915 75' '3 4 105 905'; do
      set -- $spec
      node="/adc-joystick/axis@$1"
      test "$(fdtget -t i ${dtb} "$node" reg)" = "$1"
      test "$(fdtget -t i ${dtb} "$node" linux,code)" = "$2"
      test "$(fdtget -t i ${dtb} "$node" abs-range)" = "$3 $4"
      test "$(fdtget -t i ${dtb} "$node" abs-flat)" = 32
      test "$(fdtget -t i ${dtb} "$node" abs-fuzz)" = 4
    done
    mux="$(fdtget -t x ${dtb} /adc-mux phandle)"
    test "$(fdtget -t x ${dtb} /adc-joystick io-channels)" = "$mux 0 $mux 1 $mux 2 $mux 3"
    test "$(fdtget ${dtb} /adc-mux channels)" = 'left_x right_x left_y right_y'
    modinfo -F vermagic ${module} | grep '^${kernel.modDirVersion} '
    test "$(modinfo -F name ${module})" = adc_joystick
    mkdir -p "$out"
    cp ${dtb} "$out/rg353m.dtb"
    echo 'RG353M directions, calibrated ranges, filtering, polling and ADC module passed'
  ''

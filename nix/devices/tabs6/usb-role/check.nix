# tabs6-usb-role against a temporary sysfs tree and SM5705 register file
# (usb-role.test.py), plus shellcheck. No hardware is touched.
{ pkgs }:
pkgs.runCommand "tabs6-usb-role-check"
  {
    nativeBuildInputs = [
      pkgs.python3
      pkgs.bash
      pkgs.coreutils
      pkgs.shellcheck
    ];
  }
  ''
    shellcheck -s bash ${./tabs6-usb-role.sh} ${../bluetooth/tabs6-bt-pair.sh}
    python3 ${./usb-role.test.py} ${./tabs6-usb-role.sh}
    touch $out
  ''

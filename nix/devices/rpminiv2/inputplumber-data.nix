{ pkgs, inputplumber }:

import ../../base/inputplumber-data.nix {
  inherit pkgs inputplumber;
  name = "rpminiv2-inputplumber-data";
  src = ./inputplumber;
  deviceCheck = ./inputplumber-check.py;
}

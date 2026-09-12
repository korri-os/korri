{ pkgs, inputplumber }:

import ../../base/inputplumber-data.nix {
  inherit pkgs inputplumber;
  name = "rgds-inputplumber-data";
  src = ./inputplumber;
  deviceCheck = ./inputplumber-check.py;
}

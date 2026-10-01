{ pkgs, inputplumber }:

import ../../base/inputplumber-data.nix {
  inherit pkgs inputplumber;
  name = "h700-inputplumber-data";
  src = ./inputplumber;
  deviceCheck = ./inputplumber-check.py;
}

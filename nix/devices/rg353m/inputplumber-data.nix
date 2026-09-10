{ pkgs, inputplumber }:

import ../../base/inputplumber-data.nix {
  inherit pkgs inputplumber;
  name = "rg353m-inputplumber-data";
  src = ./inputplumber;
  deviceCheck = ./inputplumber-check.py;
}

# Verify the actual diagnostic package selected by the device, not a global overlay.
{ pkgs, configuration }:
let
  lib = pkgs.lib;
  selected = lib.findFirst (
    p: lib.getName p == "v4l-utils"
  ) null configuration.config.environment.systemPackages;
  global = configuration.pkgs.v4l-utils;
  expected = global.override { withGUI = false; };
  closure = pkgs.closureInfo { rootPaths = [ selected ]; };
in
assert selected != null;
assert selected.outPath == expected.outPath;
assert global.outPath != selected.outPath;
assert configuration.config.hardware.graphics.enable;
pkgs.runCommand "rg353m-diagnostics-check" { } ''
  for command in v4l2-ctl media-ctl ir-keytable; do
    test -x ${selected}/bin/$command
  done
  ${selected}/bin/v4l2-ctl --version
  test ! -e ${selected}/bin/qv4l2
  test ! -e ${selected}/bin/qvidcap
  if grep -E '/[^/]*-qt(base|declarative|wayland|5compat|shadertools|svg|translations)-' ${closure}/store-paths; then
    echo 'Qt unexpectedly retained by CLI diagnostics' >&2
    exit 1
  fi
  touch "$out"
''

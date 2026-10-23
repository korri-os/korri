{ pkgs, launcher }:
let
  runner = pkgs.writeShellApplication {
    name = "korri-portal-wpe-test";
    runtimeInputs = [ pkgs.python3 ];
    text = ''
      export GBM_BACKENDS_PATH=${pkgs.mesa}/lib/gbm
      export LIBGL_DRIVERS_PATH=${pkgs.mesa}/lib/dri
      export __EGL_VENDOR_LIBRARY_FILENAMES=${pkgs.mesa}/share/glvnd/egl_vendor.d/50_mesa.json
      exec python3 ${./tests/acceptance.py} ${launcher}/bin/korri-portal-wpe "$@"
    '';
  };
in
{
  type = "app";
  program = "${runner}/bin/korri-portal-wpe-test";
}

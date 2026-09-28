# Off-device experiment only. Does not change the approved package or provenance.
let
  flake = builtins.getFlake ("path:" + toString ../..);
  original = flake.packages.x86_64-linux.sunshine-korri;
in
original.overrideAttrs (old: {
  patches = old.patches ++ [ ./rotation-probe.patch ];
  cmakeFlags = old.cmakeFlags ++ [ "-DCMAKE_CXX_FLAGS=-DSUNSHINE_CAPTURE_ROTATION_PROBE" ];
})

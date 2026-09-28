{ pkgs }:
# Separate test-only closure. Never selected by a production service or image.
let
  # Separate output: never include this uinput/EVIOCGRAB test in the device closure.
  grabDeliveryTest = pkgs.stdenv.mkDerivation {
    pname = "korri-native-grab-delivery-test";
    version = "0.0.0";
    dontUnpack = true;
    nativeBuildInputs = [ pkgs.pkg-config ];
    buildInputs = [ pkgs.systemd ];
    buildPhase = ''
      cp ${./native-replay.c} native-replay.c
      cp ${./native-grab-test.c} native-grab-test.c
      $CC -std=c11 -O2 -Wall -Wextra -Werror $(pkg-config --cflags libsystemd) \
        native-grab-test.c $(pkg-config --libs libsystemd) -o korri-native-grab-delivery-test
    '';
    installPhase = ''
      mkdir -p $out/bin
      cp korri-native-grab-delivery-test $out/bin/
    '';
  };
  portal = (import ../../portal/nix/package.nix { inherit pkgs; }).overrideAttrs (old: {
    pname = "korri-portal-native-input-counters";
    nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ pkgs.python3 ];
    postPatch = (old.postPatch or "") + ''
      python3 ${./instrument-native.py} .
    '';
  });
in
(import ../package.nix { inherit pkgs; }).overrideAttrs (old: {
  pname = "korri-portal-native-input-probe";
  patches = (old.patches or [ ]) ++ [ ./shell.patch ];
  nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [
    pkgs.pkg-config
    pkgs.python3
  ];
  buildInputs = (old.buildInputs or [ ]) ++ [ pkgs.systemd ];
  postPatch = (old.postPatch or "") + ''
    cp ${./native_input_diagnostic.rs} src/input_diagnostic.rs
  '';
  postInstall = (old.postInstall or "") + ''
    cp ${./native-replay.c} native-replay.c
    $CC -std=c11 -O2 -Wall -Wextra -Werror $(pkg-config --cflags libsystemd) \
      native-replay.c $(pkg-config --libs libsystemd) -o $out/bin/korri-replay-native-dpad
    mkdir -p $out/share/korri-native-input-probe
    ln -s ${portal} $out/share/korri-native-input-probe/portal
    cp ${./native-verdict.py} $out/share/korri-native-input-probe/native-verdict.py
    cp ${./NATIVE-README.md} $out/share/korri-native-input-probe/README.md
  '';
  doInstallCheck = true;
  installCheckPhase = ''
    $out/bin/korri-replay-native-dpad --self-test
    mkdir native-parser-tests
    cp ${./native-verdict.py} native-parser-tests/native-verdict.py
    cp ${./native-verdict.test.py} native-parser-tests/test_native.py
    python3 -m unittest discover -s native-parser-tests -p 'test_*.py'
    # The real EVIOCGRAB check needs an isolated off-device VM, not this sandbox.
  '';
  passthru = (old.passthru or { }) // {
    inherit portal grabDeliveryTest;
  };
})

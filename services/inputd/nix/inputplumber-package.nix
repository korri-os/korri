{
  inputplumber,
  dbus,
}:

let
  version = "0.75.2";
  sourceHash = "sha256-KiSroDcaWvzr5sP0jzr1GFyk0lHbtCFJrP3g5/b3hLQ=";
  cargoHash = "sha256-VwQ38Jv5OvyBqo9BBTnpUjgNwAbWyIdUKFKXsGC6+Mo=";
  patches = [
    ./inputplumber-dbus-observer-timing.patch
    ./inputplumber-target-device-paths.patch
  ];
in
assert inputplumber.pname == "inputplumber";
assert inputplumber.version == version;
assert inputplumber.src.outputHash == sourceHash;
assert inputplumber.cargoHash == cargoHash;
assert (inputplumber.patches or [ ]) == [ ];
inputplumber.overrideAttrs (previous: {
  # Keep the upstream source and dependency pins. The observer patch avoids
  # the 240 ms chord delay without changing genuine chord timing. Target node
  # discovery uses the native target-owned VirtualDevice, not identity guesses.
  inherit patches;
  # Both patches' regression tests run with the upstream unit suite on builds.
  doCheck = true;
  nativeCheckInputs = (previous.nativeCheckInputs or [ ]) ++ [ dbus ];
  DBUS_TEST_SESSION_CONFIG = "${dbus}/share/dbus-1/session.conf";
  passthru = (previous.passthru or { }) // {
    upstream = {
      owner = "ShadowBlip";
      repo = "InputPlumber";
      tag = "v${version}";
      src = inputplumber.src;
      inherit sourceHash cargoHash patches;
    };
  };
})

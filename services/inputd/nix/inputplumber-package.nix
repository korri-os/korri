{ inputplumber }:

let
  version = "0.75.2";
  sourceHash = "sha256-KiSroDcaWvzr5sP0jzr1GFyk0lHbtCFJrP3g5/b3hLQ=";
  cargoHash = "sha256-VwQ38Jv5OvyBqo9BBTnpUjgNwAbWyIdUKFKXsGC6+Mo=";
  patches = [ ./inputplumber-dbus-observer-timing.patch ];
in
assert inputplumber.pname == "inputplumber";
assert inputplumber.version == version;
assert inputplumber.src.outputHash == sourceHash;
assert inputplumber.cargoHash == cargoHash;
assert (inputplumber.patches or [ ]) == [ ];
inputplumber.overrideAttrs (previous: {
  # Keep the upstream source and dependency pins. This patch prevents DBus
  # observation from extending a gamepad release by the chord scheduler's
  # 240 ms delay. Genuine chords keep their existing timing.
  inherit patches;
  # The patch's regression tests run with the upstream unit suite on builds.
  doCheck = true;
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

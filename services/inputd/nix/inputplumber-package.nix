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
    ./inputplumber-startup-unhide-quiet.patch
    ./inputplumber-ignore-before-reconnect.patch
    ./inputplumber-lazy-attributes.patch
    ./inputplumber-source-teardown.patch
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
  # The startup unhide reloads and re-triggers udev only when it undid a hide:
  # upstream ran a full `udevadm trigger` on every start, which on the RG353M
  # (2026-10-01) queued 628 change events behind boot coldplug.
  # Resolve ordered ignore policy before a persistent composite can reclaim
  # a source slot; profile ordering must also hold across device reconnects.
  # Match only requested sysfs attributes. Enumerating all ancestor attributes
  # blocked the Tab S6 manager queue for ~40 s before Kishi matching, then ~50 s
  # before target creation. Inputd's one-second retries could not make progress
  # while the composite awaited those queued target requests.
  inherit patches;
  # Retire each source task before reusing its slot; late completion messages
  # must not clear a replacement source that now owns the same eventN.
  # Run the upstream unit suite and the patches' unit regressions on builds.
  # Reconnect exclusions need kernel/udev/DBus and run in the adoption VM.
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

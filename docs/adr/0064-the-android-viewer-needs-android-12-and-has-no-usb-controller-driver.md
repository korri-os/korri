---
status: accepted
---

# The Android viewer needs Android 12 and has no USB controller driver

**Owner decisions, 2026-11-11.** ADR 0062 ends with a viewer that has no Java
of Korri's. On Android that needs two cuts.

## Decision

- The minimum Android version rises from 5.1 (API 22) to 12 (API 31). The
  owner allowed any rise that has a reason. The agent chose 12, and the owner
  can overturn it.
- The viewer drops Artemis's own USB driver for Xbox and Switch Pro
  controllers. It uses Android's controller support.

## Why

Research on 2026-11-11 found that Rust reaches every Android feature the
viewer uses without Java source, but at different Android versions: audio
output at 8, decoder callbacks at 9, the low-latency decoder flag and the
frame-rate hint at 11, and one API for controller rumble, sensors and battery
at 12. At Android 12 the viewer has one code path for all of them.

The bench phone is a Galaxy Z Fold3. The owner named a Pixel 3 first and could
not find it. The bench proves the Android version that the Fold3 runs. That
version is not checked on this date, and it must be 12 or newer.

The USB driver is 8 Java files. No Rust version exists, and a proof needs
real controllers on a cable.

## Consequences

- A phone below Android 12 cannot run Korri.
- On some devices a cabled Xbox or Switch Pro controller can lose rumble or
  the Guide button. Which devices is not known.
- Decoder lists still come from Android's own classes, called from Rust,
  until Android 16 (API 36) is the minimum.

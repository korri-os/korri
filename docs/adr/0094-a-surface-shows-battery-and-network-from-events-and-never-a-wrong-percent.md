---
status: accepted
---

# A surface shows battery and network from events, and never a wrong percent

**Owner decisions, 2026-11-20.** Velvet showed the clock on home and in the
pause panel, but no Wi-Fi or battery. The battery reached a surface only on
Android, inside the gameplay overlay. korrid read the network every 2 s.

## Decision

- **The surface treaty carries one battery fact.** `SurfaceModel.battery` is
  `{ percent?, charging }`, beside `network`. The shape is legacy
  `DeviceBatteryReady` (percent, charging) with `percent` made optional. The
  field is absent when the device has no battery or korrid cannot read it. A
  surface cannot tell those two cases apart, and the owner accepts that.
  `SurfaceGameplayOverlayPresentation.battery_percent` is removed in the same
  cut. The Android app fills `SurfaceModel.battery` from `BatteryManager`.
- **korrid never publishes a percent that can be stale.** On Linux, korrid
  reads `/sys/class/power_supply` once at start and then only on a kernel
  power_supply uevent. A driver that sends no uevent when the capacity moves
  would leave the percent stale. Each device's Nix module therefore lists the
  battery facts it has verified in `services.korriProduct.battery.verified`,
  following `services.korriProduct.sleep.states`. The only value is
  `"percent"` and the default is empty. Without it korrid publishes only
  `charging`.
  - Verified from driver source: `qcom_battmgr` (Thor) and `qcom_fg` (Retroid
    Pocket Mini V2) call `power_supply_changed()` on capacity changes. The
    Tab S6's `sm5705_fuelgauge` gets the same change in this work.
  - `axp20x_battery` (H700, RG35XX Pro, RG35XX SP) and `rk817_charger`
    (R36T Max, RG353M, RG DS) fire only on plug or unplug, or never. Those
    devices show the charging state only.
- **No timers in korrid for these facts.** The network comes from
  NetworkManager's D-Bus `PropertiesChanged` signals, and the 2 s loop goes
  away. A model publishes only when the connection, the network list or the
  0 to 4 bars change. Onboarding status publishes when korrid's own
  onboarding state changes.
- **Velvet shows one cluster in the top-right corner of every screen** (home,
  pause, list pages, the detail page, the game set), at one height: clock,
  Wi-Fi, battery, then Menu where present. The battery is a glyph and the
  percent, with a bolt while charging. It is in Exit colour at 15% and below
  unless charging. The network glyph shows 0 to 4 bars, wired, or struck-out
  Wi-Fi when offline, and nothing when `network` is absent. A narrow window
  drops the percent first, then the clock; the glyphs stay. The pause panel's
  Battery readout goes away.

## Consequences

- Exit colour now also marks a low battery, which widens what the one signal
  colour means.
- Six devices show no percent until their battery drivers send capacity
  events. Fixing that is a kernel patch per driver; it needs no change to
  korrid or a surface, only the device's `battery.verified` list.
- The Wi-Fi page's signal percent can trail NetworkManager by a few points,
  because a change inside one bar does not publish.

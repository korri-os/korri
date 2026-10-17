# The cascade gains a device layer and modifier-keyed records

Before this, the launch cascade folded family → system → runner → game →
override. Every layer held records keyed by runner or family
(`services/korrid/src/config/cascade.rs`). A launch modifier is neither. On
2026-10-17 the owner chose this shape:

- `modifiers:` is a new top-level section of `device.yaml`, beside `runners:`
  and `families:`. It is the device layer, and it selects which modifiers apply
  to every launch. Enabling the Gamescope plugin alone does not turn it on, so a
  device can have the plugin installed but off.
- The same `modifiers` map also appears in `systems.<id>`, in `library.<id>` in
  `catalog/library.yaml`, and in the top-level `runners.<id>` record in
  `device.yaml`, but not under `systems.<id>.runners` or `library.<id>.runners`.
  Discovery-v2 replaced the games schema with library records.
- Records are keyed by contribution ID, as runner records are, for example
  `'@korri:gamescope/gamescope'`.
- A record holds `enabled` (on or off), `settings` (typed values named after
  the modifier's native options) and `config`. A modifier record may use only
  `config.append`. `prepend` and `replace` are refused.
- Layers fold device → runner → system → game. Each key, including `enabled`,
  is last-wins in either direction. An absent `enabled` means off.

The names are taken from the existing runner record (`settings`, `config`) and
from plugin enablement in `config/settings.rs` (`enabled`). They are not taken
from legacy's `launch.with` policy (ADR 0006). The same change removes the
legacy `launch.with` map from `HostPayload.launch` and `InheritableLayer.launch`.
main read those fields only to reject them, and they would otherwise be a
second, dead home for Gamescope policy.

## Considered Options

- **Enabled plugin means on everywhere.** This needs no new layer, but it
  cannot express "installed, off by default".
- **Gamescope settings inside each runner's settings.** This keeps the cascade
  shape, but the settings repeat for every runner, and a runner's settings stop
  being only its own.
- **All three config parts, as for runners.** `replace` could remove the fixed
  `--backend wayland` and bring back the DRM-master failure under Sway.

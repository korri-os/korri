# Sunshine settings are launch settings, applied before the game starts

**Owner decision, 2026-10-25.**

Sunshine reads its settings only when it starts, and restarting it ends the
session it streams (observed on the RG35XX SP, 2026-10-25). We decided that
Sunshine settings resolve through the launch settings like every other
setting, so a device, system, game or profile can set them, and korrid
applies them by restarting Sunshine before the game starts, when no viewer is
connected yet.

## Considered options

- **One set per device, applied only when no session runs.** Rejected: it
  needs a new section outside launch settings and allows no per-game values.

## Consequences

- A launch on a device with Sunshine can take a few seconds longer.
- Where Sunshine's settings sit in a launch-settings place is decided
  separately (open in `docs/briefs/2026-10-25-settings-vocabulary.md`).

# Sunshine settings are launch settings, applied before the game starts

**Owner decision, 2026-10-25.**

Sunshine reads its settings only when it starts, and restarting it ends the
session it streams (observed on the RG35XX SP, 2026-10-25). We decided that
Sunshine settings resolve through the launch settings like every other
setting, so a device, system, game or profile can set them, and korrid
applies them by restarting Sunshine before the game starts, when no viewer is
connected yet.

Sunshine's settings sit under a new place-body kind, `services`, keyed by the
service id: `services: {'@korri:sunshine/sunshine': {settings: …}}`. It has
the storage shape of a modifier but no `enabled` switch and no
`launch.compose` call. For settings changes, korrid restarts Sunshine only
when the resolved settings differ from the ones it last applied.

The plugin host separately owns Enabled lifecycle (#397, #400 part 1).
It writes runtime `korrid.service.upholds/` links for approved Enabled native
units. systemd keeps Sunshine running after a korrid or compositor restart,
a NixOS switch, or a plugin bundle replacement. Disable and replacement
withdraw these links and reload systemd before stopping the old units.
Disabled plugins have no runtime unit or uphold link. This does not change
the settings-change restart above, and needs no korrid startup reconcile.

## Considered options

- **One set per device, applied only when no session runs.** Rejected: it
  needs a new section outside launch settings and allows no per-game values.
- **Sunshine as a modifier.** Rejected: a modifier wraps the game's command,
  and its `enabled` switch would need a meaning nobody has decided.
- **Restart before every launch.** Rejected: every launch would pay the
  restart.

## Consequences

- A launch on a device with Sunshine can take a few seconds longer.
- ADR 0028's list of place-body kinds gains `services`.

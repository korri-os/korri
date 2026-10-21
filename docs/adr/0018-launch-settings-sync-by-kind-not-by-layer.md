# Launch settings sync by kind, not by layer

**Owner decision, 2026-10-21.** Sync itself is not built. This records the
rule it must follow.

Launch settings can be set on the device, a system, a runner, a game, a release
or a profile. Some values describe how a person likes to play (a runner choice,
a video filter). Others describe what one device's hardware can take (CPU and
GPU frequency limits, the CPU governor, uclamp). A game's CPU limit on an
RG353M means nothing on an Odin 2 Portal, but a person's preferred filter
does.

## Decision

- Each kind of launch setting is marked either shared or device-only. The mark
  belongs to the kind, never to the layer: a game-level or profile-level value
  of a device-only kind stays on its device.
- Every CPU and GPU setting in the first launch-settings slice is device-only.
  That slice therefore ships without any sync.
- Sync of shared kinds is a later slice. Today each device reads its own
  `device.yaml` and `catalog/library.yaml`, and no peer path writes them.

## Considered options

- **Sync by layer** (for example, game and release layers sync and the device
  layer does not). Rejected: a game-level CPU limit would travel to hardware it
  was never measured on.
- **Never sync launch settings.** Rejected by the owner: preferences such as how
  a person plays a game should follow them across devices.

## Open questions for the sync slice

- Shared values must be keyed by game identity, not by a device's library ID.
- Ownership (person or device owner), transport (the saves peer path is the
  nearest pattern, ADR 0014), and edits that two devices make while offline.
- When a synced value and a local value meet on the same layer, which wins.

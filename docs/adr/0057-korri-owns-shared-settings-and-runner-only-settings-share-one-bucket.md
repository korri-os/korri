---
status: accepted
---

# Korri owns shared settings, and runner-only settings share one bucket

**Owner decision, 2026-11-09.** Amends ADR 0028 (which cells take settings)
and ADR 0037 (rule R2 and who declares shared sections). Nothing here is
implemented yet.

ADR 0037 gave every plugin one vocabulary, but each plugin still declared the
shared sections itself in `settings.describe`, and a plugin could add its own
section named for any concept no other plugin had (R2). ADR 0028 allowed
`settings` only in family and runner cells. So a value such as "draw every
GameCube game at 4x" had to be written once per runner, nothing stopped two
plugins from giving one shared path two ranges, and a reader could not tell a
shared setting from a runner-only one by its name.

## Decision

- **Korri owns every shared setting**: its path, value shape, range and
  meaning. A plugin declares which shared paths it supports and translates
  them into its program's own settings. It does not redeclare them.
- **Korri also decides which shared settings stay with each device**
  (`x-korri-scope: device`, ADR 0018), because only Korri knows a shared
  setting's meaning. Settings that depend on the device's GPU or display,
  such as render scale, stay device-only, so a value chosen on a desktop
  never reaches a handheld.
- **Shared settings may be set at any runner scope**, including the
  any-runner cells of the device, system, game, release, profile and one
  launch. Whichever runner is chosen translates them.
- **Settings only one runner has live in one bucket whose name is the same for
  every runner.** It is `plugin`, kept from legacy's `settings.plugin`, and it
  serves modifiers and launch services as well as runners. The bucket is
  limited to one plugin by the cell it sits in and is never translated. This
  replaces R2's own sections named for the concept. A concept that later
  becomes shared moves out of the bucket into a shared path by a one-off
  migration.
- **A runner that cannot honour a shared value skips it** and the launch goes
  ahead with a warning that names the path, as ADR 0037 does for an unknown
  setting. The editor marks the value "not supported by" that runner.
- **A runner whose program takes a different or finer shape maps to its
  nearest supported value** and warns when it had to round, for example a
  shared 4x render scale on a runner that takes a percent or only some steps.
- **Shared settings use human units**: volume as a percent, render scale as a
  multiplier, aspect and modes as named choices. Each runner converts to its
  program's unit.
- **A modifier's way of doing the same thing stays a separate setting.** A
  runner's frame-rate cap or scaling changes the game itself; Gamescope's
  changes only what is presented. The shared setting is the runner's; the
  modifier keeps its own settings in its own cell, and the editor says which
  one does what.

## Considered options

- Keep R2's own sections named for the concept. Rejected: shared and
  runner-only settings look the same, and two runners can each invent the
  same section name with different meanings.
- Keep settings in family and runner cells only. Rejected: a shared
  vocabulary that must still be repeated per runner gives most of its value
  away.
- Let each plugin keep declaring shared paths, aligned by review. Rejected:
  the drift ADR 0037 was written to stop.
- Skip silently (legacy) or refuse the launch. Rejected: silence hides why a
  value did nothing; refusal lets one broad system value block every game
  whose runner lacks it.
- One shared setting for the outcome, with Korri choosing whether the runner
  or a modifier carries it out. Rejected: the two behave differently, and
  Korri would be choosing between them for the person.
- Exact match or skip. Rejected: broad values would do nothing on many
  runners.

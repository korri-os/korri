---
status: accepted
---

# A game has one launch-settings list, wherever it runs

**Owner decisions, 2026-11-11.** A design interview on the parts that ADR 0056
and ADR 0057 left unbuilt. The owner judged a prototype of the result, kept on
branch `proto/one-list-launch-settings`, and accepted it.

The first editor showed the stored places one after another, and then a
separate "This launch" block that repeated each setting. A streamed game showed
no settings at all on the device that plays it. The owner found the separate
block overbearing and its controls duplicated, and wants "the same config, no
matter if it's a local or if it's a remote stream".

## Decision

- **One list.** Each setting appears once, grouped by setting section, with the
  value the next launch would use. There is no block for each place and no
  separate block for one launch.
- **One choice of where a change is kept.** A changed row is marked and shows
  its earlier value. One control then asks where the changes are kept: this
  launch, this game, or every game. This game is the default.
- **A streamed game shows the same list.** The device that runs the game holds
  and applies its launch settings. The device that plays it reads and writes
  them there, through the calls that a device of the same owner may already
  make. Nothing is copied between devices for this. The list says where the
  game runs. Which device keeps a value is a note on the focused row, never a
  second list.
- **What describes the playing device stays with it.** The values of its stream
  viewer, such as stream size, frame rate, bitrate and codec, are that device's
  own launch settings. They appear in the same list, as its stream section.
- **One-launch values travel with a streamed launch**, in the same launch
  request. Only a device of the owner of the device that runs the game can
  send them.
- **One launch also carries the CPU governor and launch service settings.** It
  never carries hooks, environment variables or arguments; ADR 0029 holds.
- **A game that runs on the phone gets exactly the settings its runner
  declares** (ADR 0057), for every kind of Android app in ADR 0054. A runner
  that can honour none shows none. There is no rule only for Android.
- **"Streamed game" replaces "far game"** in code, contracts, documents and
  issues.

One part is an implementation choice, not an owner decision: a one-launch
value has no runner scope. A shared setting applies to the launch's runner, a
runner-only setting names its runner, and the scope is chosen when the value
is kept. This removes the limit of one scope for each launch.

## Considered options

- **Two lists for a streamed game**, one for the device that runs it and one
  for the device that plays it. Rejected by the owner: it splits one config.
- **Keep the separate one-launch block.** Rejected by the owner: every setting
  appears twice.
- **Copy the settings to the playing device and edit them there.** Rejected:
  the values would not act where the game runs. Copying waits for records
  (ADR 0018, as amended on 2026-11-11).
- **Typed arguments or hooks for one launch.** Rejected: with one-launch values
  travelling between devices, a phone could make another device run a command
  (ADR 0029).

## Open

- How the system, release and profile places are reached from the one control.
- Pico's form of the list.
- The phone's stream viewer values as Korri launch settings (issue #395). The
  viewer that Korri owns (issue #391) changes which of these settings exist.

## As built (2026-11-12, issue #445)

These are implementation choices. They follow the decision above and can
change without a new owner decision.

- **One-launch wire.** `LaunchOverride` holds shared values in `settings`
  with no runner scope, runner-only values in `plugin` keyed by runner id,
  `modifiers` keyed by modifier id, `services` keyed by service id, the CPU
  limits with `governor` beside them, and the GPU limits. korrid refuses a
  `settings` key that is not a shared setting.
- **The streamed launch request.** `SessionPrepareRequest` has the same
  optional `launch` as `SessionStartRequest`. A prepare that carries it has
  the owner-device policy; a pass with the stream scope cannot send it.
- **Which device answers.** The launch-settings calls, the profile calls,
  `GameRoutes` and `SelectedGameLaunch` take an optional `holder`: the
  label of the device that runs the game. `SessionPrepareRequest` keeps its
  older field `host` for the same thing; `AGENTS.md` forbids extending that
  name, so the new fields do not copy it. A local caller names the holder
  and its own korrid asks that holder, in host mode and in brain mode. A peer
  cannot use `holder` to make a korrid ask a third device.
- **Keeping.** The editor writes the changes when it closes, or when its game
  starts from the runner chooser. A shared value goes to the any-runner
  place and a runner-only value to its runner's place. The CPU governor is
  one device value: it is kept on the device whatever place is chosen, or
  named for one launch. The game starts only when every change that waits
  was written. When a write fails, the editor stays open in Error or
  Conflict with its changes marked, and nothing starts.
- **One-launch governor.** The limits helper sets the governor for the whole
  device. korrid applies the launch's governor when the game unit starts and
  puts the device's own back when it ends. A korrid restart also applies the
  device's own.
- **The stream section.** The playing device's stream viewer values join
  the changes that wait, like every other row. A changed stream row is
  marked with its earlier value and counts in the keep control. It is
  written when the editor closes or the game starts, and a write that fails
  stops the start. It is always written to the playing device, in its device
  place for the viewer, whatever place the keep control names. The value
  describes that device, and that device has no game place and no one-launch
  place for a streamed game's viewer (issue #395). When the changes are for
  this launch, the keep control says that stream values stay on this
  device. When the holder cannot be reached, the stream rows still change,
  and the Close action still writes them.
- **A runner choice for a streamed game.** The chooser reads the holder's
  runners. A holder with one runner selected prepares as before, with the
  viewer's display. A runner chosen for one launch uses `SelectedGameLaunch`,
  which carries no viewer display; the live shape report sets the shape after
  the viewer connects (ADR 0058).
- **Not in the list.** Values that live only in files (hooks, environment,
  arguments, working folder, raw config) and the runner's prepare facts are
  not rows. `korri launch-settings show` prints them.

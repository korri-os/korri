---
status: accepted
---

# Runner settings follow the display, win over native files, and apply at launch

**Owner decision, 2026-11-09.** Extends ADR 0028 and ADR 0037.

On 2026-11-09 Mario Kart Wii: Retro Rewind (WiiCompiled) streamed to a
2448×1848 phone with black bars. The game kept the 854×480 window saved in its
own `UserData/Config.toml`. Korri passed the display size to the Gamescope
modifier (`PluginComposeInput.display`) but not to the runner
(`PluginLaunchInput` has no display), and WiiCompiled reads its video settings
only from that file, which its in-game F10 menu also rewrites.

## Decision

- **An unset size follows the display.** When no place sets a size-like
  setting, the runner follows the display that this launch's composition
  uses. Korri gives runners that display fact, as it already gives it to
  modifiers. A value that a place sets always wins. There is no symbolic
  value such as `auto`.
- **Values set in Korri win over the program's own config file.** When a
  runner can only apply a setting by writing the program's native config
  file, it writes every value set in Korri at every launch. It leaves every
  other key in that file as the program or its menu left it. A change made in
  the game's menu to a value Korri sets lasts only until the next launch, and
  the editor says so.
- **Values derived from the display count as values set in Korri.** A runner
  that can follow the display only by writing a size into the native file,
  such as WiiCompiled's `window_width` and `window_height`, writes it at every
  launch.
- **Settings apply at launch.** Korri does not change a running game's
  settings. The game's own menu stays the way to change a value mid-session.

## Temporary native values

**Owner decision, 2026-11-10 (q38).** One-launch values must not remain in a
program's native config after that launch. Restore the previous values after
the game ends, but preserve unrelated settings and later menu edits. Keep
permanent game and system settings as direct writes. Track temporary writes
so an interrupted launch can be recovered before another launch uses them.

A test with the real WiiCompiled callback and korrid showed the conflict:
starting at 1x, a one-launch 4x write left 4x in Config.toml on a second launch
with no override. Consuming the in-memory override was not sufficient.
Leaving those native writes sticky was considered and rejected. A direct
write is still permitted; restoration is the additional obligation for a
one-launch value.

## Considered options

- A symbolic `auto` value on size settings. Rejected: a magic value in every
  size-like setting, and the cascade would have to know which value means
  "ask the display".
- Seed the native file once and let the in-game menu own it afterwards.
  Rejected: after the first launch the launch settings would no longer say
  what runs, and a per-game value could not override a value the menu wrote.
- Write only a separate per-launch config file. Rejected: WiiCompiled and
  every program without a config-path option could not take settings at all.
- A launch-scoped overlay of the native file inside the launch's bubblewrap
  namespace (ADR 0015). Rejected for now: discarding the overlay loses every
  change made in the game's menu, and copying changes back means korrid must
  parse each program's config format.
- Live changes to a running game. Deferred: they need a runtime channel to a
  running runner, which the effect-free `launch.prepare` model does not have.

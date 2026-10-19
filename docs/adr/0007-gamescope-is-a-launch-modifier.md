# Gamescope is a per-launch modifier, not the device compositor or korrid code

Gamescope wraps one game launch, nested inside the device's existing
compositor. korrid starts it around the game and owns its lifetime, and the
portal and Sway stay as they are. It arrives as an ordinary plugin through the
`launch.compose` operation from `docs/briefs/2026-09-15-plugin-model/`, so
korrid holds no Gamescope knowledge.

## Considered Options

- **Gamescope as the device compositor**, as in Steam Deck mode. It avoids
  nesting, but it replaces Sway for the portal and every game. That is a
  device-image change, not a launch feature.
- **Gamescope wiring inside korrid's launcher.** It is faster to build, but it
  pins one program's knowledge in the core, against the plugin boundary rule.

## Consequences

`launch.compose` runs in the same `korrid plugin-launch` process as
`launch.prepare`, after it and before `exec`. Gamescope is then the main
process of the session's systemd unit, and the game is its child, so freezing,
raising and ending the live session act on the unit as a whole. No reaper
searches for Gamescope processes by name.

Gamescope is the first and only consumer of `launch.compose`. The operation
gets what Gamescope needs and nothing more; ordering among several modifiers
waits for a second real modifier. Every wrapped launch carries a nested
compositor and its latency.

## Runtime and diagnostic ownership

A modifier launch uses a real systemd `RuntimeDirectory` as `XDG_RUNTIME_DIR`.
The runtime user owns it, and systemd removes it on stop, including after
SIGKILL. korrid binds its separate diagnostic directory at the runtime's
`diagnostics/` subpath. The published Wayland socket is bound there too.

Do not bind diagnostics over `XDG_RUNTIME_DIR`: games can create runtime-user
0700 subdirectories such as `dconf/`, which korrid cannot traverse to clean up.
The diagnostic directory keeps its sticky bit and named-user ACLs, so the
runtime user can create redaction words but cannot replace or write korrid's
stderr file. systemd still opens stderr through the host source before it
applies mount namespaces. This split needs no new korrid privilege or
`ExecStopPost` cleanup command.

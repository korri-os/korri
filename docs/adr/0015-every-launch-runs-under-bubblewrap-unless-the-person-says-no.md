# Every launch runs under bubblewrap unless the person turns it off

**Owner decision, 2026-10-21.**

korrid runs every launch through bubblewrap inside the launch's systemd scope,
unless the person's settings turn it off for a system, runner or game. The
namespace shows the program the host as it is and changes only the binds and
overlays that the runner plugin declares for that launch (ADR 0014). It is a
mount view, not a sandbox. Runners cannot declare that they must not be
wrapped; only the person's settings decide, as with Gamescope (ADR 0008).

If bubblewrap cannot start, Korri refuses the launch and reports why. It never
starts the game without it: an unwrapped launch would write saves to the wrong
place without a warning.

Korri's default settings turn bubblewrap off for the Steam runner, and the
person can see and change that. Steam starts its own bubblewrap
(pressure-vessel) for each game, and Steam saves are left to Steam Cloud
(ADR 0014).

Gamescope stays outside bubblewrap: Gamescope, then bubblewrap, then the game.
bubblewrap sets `no_new_privs`, so a Gamescope started inside it would lose the
file capability that gives it real-time priority.

## Considered Options

- **Only launches that declare a bind or overlay.** Smaller risk at first, but
  two launch paths to keep working.
- **A transient service with systemd `BindPaths=`.** A scope cannot take it
  (systemd.scope(5)), switching to a service changes how korrid tracks freeze,
  thaw and exit, and `BindPaths=` has no overlay.
- **korrid builds the namespace itself with unshare and mount.** No extra
  program, but korrid would reimplement bubblewrap's edge cases.
- **This decision.** One launch path, one rule the person can see.

## Consequences

Every runner carries the same risk at once. Before this ships, a test on the
AYN Odin 2 Portal or the Retroid Pocket Mini V2 must show that FEX-run x86
programs start and render inside the namespace: FEXServer, the FEX root
filesystem and the x86 `binfmt_misc` handler must all still work. Until then
the risk is unknown, not known to be safe. bubblewrap becomes part of the
product module on every device. NixOS allows unprivileged user namespaces by
default (`security.allowUserNamespaces`), and nixpkgs' bubblewrap is not
setuid. A runner that breaks under bubblewrap fails at launch until the person
turns it off for that runner.

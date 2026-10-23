# Hooks, environment and arguments run as the game and live in files

**Owner decision, 2026-10-22.** The phase that applies it is in design.

The one launch-settings cascade (ADR 0028) carries hooks, environment variables,
extra arguments and the working directory. Each of these runs code or changes
what runs. Legacy ran hook steps unsandboxed as the session user, and its CPU
hook used `sudo -n`.

## Decision

- Each hook step runs as the game's runtime user, in its own short transient
  unit with the game unit's sandbox. It is never root. A hook can do what the
  game could do. Clock and power changes belong to limitsd (ADR 0020), not to
  hooks.
- Hooks, environment, arguments, working directory and raw runner `config`
  are authored only in the device's config files. The portal shows them read-only, with the layer each
  value comes from. Editing them needs shell access to the device, which
  already means full trust.
- Layer environment overrides the plugin's `launch.prepare` environment per
  key, and a null value unsets the variable. `args.prepend` goes before the
  plugin's arguments and `args.append` after them. A launch warning names the layer that set a value
  when that value breaks a runner.
- Reusable named step lists live in a top-level `routines` section of
  `device.yaml` only. A layer
  uses one with `hooks.use`, so `hooks` always means steps that run there.
- A hook step sees the game's resolved environment: the plugin's env plus the
  layer env. Korri also sets `KORRI_HOOK_PHASE` (`before` or `after`),
  `KORRI_LAUNCH_ID`, `KORRI_GAME_ID`, `KORRI_RELEASE_ID`, `KORRI_SYSTEM_ID`,
  `KORRI_RUNNER_ID` and `KORRI_PROFILE_ID`. After-steps also get
  `KORRI_SESSION_RESULT` (`exited`, `failed` or `stopped`) and, when the game
  exited, `KORRI_EXIT_STATUS`. A variable whose value does not exist is unset,
  not empty. Nothing keeps legacy's names for compatibility.
- Hook step rules follow legacy. Before-steps run in order, and on failure
  they abort the launch or warn (default abort). After-steps run in reverse
  order and never block teardown. The default timeout is 30 seconds.

## Considered options

- Run hooks as root. Rejected: any config edit would become root code
  execution.
- Let the kiosk portal edit hooks, environment and arguments. Rejected: anyone
  holding the controller could make the next launch run a command. An
  owner-confirmed editor may come later.

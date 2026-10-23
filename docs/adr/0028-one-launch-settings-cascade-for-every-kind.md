# One launch-settings cascade for every kind of setting

**Owner decision, 2026-10-22.** The phase that applies it is in design.

On 2026-10-22 korrid had four cascades. Each had its own layers and order:

- CPU and GPU limits: device → system → runner → game → release → profile.
- Runner settings and config: family → system·family → runner → system·runner →
  game·family → game·runner → one-off override (`config/cascade.rs`).
- Modifiers such as Gamescope: device → runner → system → game
  (`config/modifiers.rs`).
- Runner choice: system, then game.

A profile could change only CPU and GPU limits, so it could not express "the way
I like to play this game".

## Decision

- All launch settings resolve in one cascade over two dimensions. The layer is
  device, system, game, release, profile or one-off launch. The runner scope is
  any runner, one runner family, or one runner. They merge in this order, and
  the later cell wins: device·any, system·any, device·family, device·runner,
  system·family, system·runner, game·any, game·family, game·runner, then the
  same three cells for release, profile and the one-off launch. A system (all
  GBA games) is broader than a runner, and a runner-scoped device value beats a
  system value for any runner.
- Every cell takes the same body: `runner` (not in a runner-scoped cell),
  `cpu`, `gpu`, `modifiers`, `env`, `cwd`, `args`, `hooks`, and in family and
  runner cells `settings` and `config`. The CPU governor stays device-only.
- `args` has `prepend` and `append` lists, merged in layer order. There is no
  replace, because replacing a runner's arguments breaks its launch.
- The one-off launch override stays as the last, most specific layer.
- Defaults that a plugin declares form a base layer under the device layer.
  The editor shows these values as "from the runner". A plugin gives defaults
  only for its own runner or modifier settings and for env and args. A setting
  default is the JSON Schema `default` keyword on that property in
  `settings.describe`. Env and arg defaults are what `launch.prepare` returns.
  A plugin never gives defaults for CPU, GPU, hooks or runner choice. Because
  plugins are effect-free, the editor calls `launch.prepare` as a dry run to
  show the runner's env and args.
- The one-off launch override carries only the kinds the portal may edit:
  runner choice, runner settings, modifiers, CPU and GPU. korrid refuses any
  other kind in an override.
- The game editor shows a cell for every installed runner that can play the
  game, beside the any-runner and family cells.
- The whole change ships as one slice: the engine, the schema and migration,
  every kind of setting, hook execution, profiles and the editor.
- Every Korri-owned key in `device.yaml` and `catalog/library.yaml` is
  kebab-case. Plugin-owned keys and the RPC wire keep their own case.
- Top-level `runners`, `families` and `modifiers` in `device.yaml` move under
  `host` by the same one-off migration.
- Any value can sit on any layer, and the most specific layer that sets it wins.
  Each kind of setting keeps its own merge rule for its value, for example
  per-key for runner settings and ordered for hook steps.
- The runner is chosen first, from the same layers. Values that are scoped to
  a runner then apply only when that runner is chosen.
- The phase that applies this rule adds runner choice, runner settings,
  modifiers, environment and arguments, and hooks to the cascade.
- If the chosen runner is not installed, the next less specific layer's runner
  choice is used. Korri shows a notice that names the missing runner, and
  values scoped to that runner are skipped.
- Every device-layer value lives under `host` in `device.yaml`. Values that sat
  elsewhere, such as the top-level `modifiers`, move there by a one-off
  migration.
- Marks (ADR 0018): runner choice is shared. Runner settings are shared unless
  the plugin marks a key `x-korri-scope: device`. Modifiers, environment,
  arguments, working directory and hooks are device-only.

## Considered options

- Keep separate cascades and add the missing layers to each. Rejected: the
  layer orders had already drifted apart. The editor would have to explain four
  rule sets.
- Runner as its own layer, with runner scope only for runner settings and
  modifiers. Rejected: CPU limits, environment and hooks could not be limited
  to one runner, and the rule would differ by kind.
- Runner broader than system. Rejected: legacy and the CPU/GPU cascade already
  put system first. The modifiers order changes, and on 2026-10-22 only a
  device-level Gamescope setting existed on any device.

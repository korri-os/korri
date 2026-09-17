# Korri (main)

This branch is a deliberate restart. Architecture and folder structure are
being decided as each end-to-end slice lands — do not assume conventions from
the `legacy` branch apply here unless this file says so.

## Rules of engagement

- `legacy` is read-only reference material. Harvest code from it deliberately;
  never merge it wholesale.
- Bring in as little as possible per slice. If a slice doesn't need it, it
  doesn't come over.
- `main` carries no backward-compatibility baggage. Do not add aliases,
  fallback reads, dual writes, compatibility branches, or runtime migrations
  for superseded Korri behavior. Make one clean cut; preserve real user data
  with an explicit, one-off operational migration when the cut is deployed.
- The platform target is Linux. Android was dropped on 2026-09-16; the tag
  `android-final` marks the last commit that carried it, and
  `docs/briefs/2026-09-16-android-removal-scope.md` records what went.
- Read before you touch. Do exactly what was asked. No bonus refactors.

## Map

```
clients/portal/    TS host: talks to the korrid brain over localhost RPC, then
                   publishes one surface model and mounts a surface; browser
                   dev uses the same in-memory korrid client and keyboard input
surfaces/          presentation surfaces. One per directory, each self-contained
                   and free to move to its own repository: a surface may import
                   contracts/surface/ (types only) and nothing else from Korri
packages/          shared, product-agnostic packages consumable by surfaces
contracts/         treaties between deployables; imports nothing outside
                   contracts/; when sides disagree, the contract file wins.
                   contracts/surface/ is hand-written; contracts/generated/ is
                   Typeshare output from Rust (read-only)
services/korrid/   Rust brain, one per device. Ships as a standalone binary
                   with a configured bind, serving capability-bound RPC on
                   localhost
```

One shared model of how Korri behaves; each platform meets it as well as it
can, and the gap is absorbed at the edge — never leaked into the core or UI.

## Federation

Korri is a federation of devices, not hosts and clients. A device is defined
by the capabilities attached to it — a screen, controllers, an encoder, fast
storage, an internet connection, the ability to run a given kind of content —
and any device may have any subset, including none. No device holds a role.

Consequences that bind design:

- Content declares what it needs; devices advertise what they can do; the
  federation matches the two. The same content may have several fulfilment
  routes (Wario Land 4 plays locally on the tablet *and* streams from a device
  running RetroArch). A route is chosen, never assumed.
- Logic that could run anywhere must not perform its own effects. Plugins,
  launchers, and acquisition return declarations — what should exist, what
  should run — and korrid performs the effect on whichever device holds the
  capability. Code that downloads, writes, or spawns directly is pinned to one
  machine forever. `services/korrid/src/launcher/` is the reference shape.
- Every device runs its own korrid. Peers are peers; direction belongs to an
  individual call, never to the architecture.

Names that predate this framing — `upstream`, host mode, `Game.host` — are
shorthand for "peer with a given capability set". Don't extend them; rename
when the surrounding code is next touched.

Guard: the capability model is deliberately unbuilt. Do not invent it ahead of
real cases. Today that is three devices and one genuinely multi-route piece of
content — shape for federation, build no more than the cases demand.

The same guard applies to every persisted-data and configuration schema.
Schemas are architecture, including examples proposed in conversation. Do not
invent paths, filenames, collection names, field names, nesting, variants,
defaults, identity rules, migrations, or fallback behavior to make a proposal
look concrete. A thin slice narrows behavior; it does not pre-decide its data
model. Introduce schema only by extracting it from an existing producer,
consumer, treaty, or observed real record, or after the user explicitly chooses
it. Cite that grounding when proposing or implementing the schema. If no such
grounding exists, state that the schema is unresolved and ask which real case
or existing data should define it; do not fill the gap with illustrative YAML,
JSON, types, or pseudo-records.

Legacy's schema design is the presumptive baseline when it covers the case at
hand. Extract and preserve it deliberately; do not restate, simplify, rename,
or redesign it as part of moving it to `main`. In design discussion, present
only specific alterations for which there is a concrete idea or concern. Name
the legacy element, the proposed delta, and why it may be warranted. When there
is no such concern, propose no schema change.

## Plugin boundaries

Keep `plugin.ts` thin. It declares the plugin's identity and Korri integration,
not a replacement configuration language for the systems the plugin uses.
This rule applies to every plugin and platform, not only Tailscale.

- Put system-specific configuration in its established native format. A Linux
  daemon belongs in an actual systemd `.service` file shipped with the plugin.
  Do not encode service directives in TypeScript or JSON merely so Korri can
  translate them back into a service file.
- Use the declaration to register contributions and reference their native
  artifacts. Each artifact owns its configuration. Do not keep a second,
  hand-maintained representation in `plugin.ts` or a Korri-specific schema.
- Generate derived packaging and discovery metadata from those sources. Plugin
  authors must not maintain duplicate manifests to keep the representations
  aligned.
- Korri still owns artifact validation, permission approval, lifecycle and
  effects. A native configuration file does not grant permission to bypass the
  host's security policy. Plugin declaration code remains effect-free.
- Native programs arrive prebuilt. Native configuration and interpreted plugin
  declarations do not make compilation on a target device acceptable.

This is the required design direction. It does not claim that the current host
already accepts raw service files. Changing that runtime boundary requires its
own implementation and verification; this rule does not authorize a silent
permission expansion.

## Standing decisions

- The portal is hardware-blind: it receives semantic input actions
  (`direction`, `confirm`, `back`, …), never key codes. `clients/portal/src/
  input/` translates every device Korri supports.
- `flake.nix` is an index: inputs + per-area composition only. Shared
  toolchain composition lives in per-area Nix expressions; `devshell.nix`
  owns the interactive shell. No inline derivations or shells.
- Project tasks are Nix apps; discover them with `nix run .#help`.
- A pre-commit hook refuses a commit that stages a secret. Run
  `nix run .#hooks-install` once per clone; Git shares the hook with every
  worktree. `lefthook.yml` declares it and `.gitleaks.toml` holds the rules,
  including the path rule for korrid device keys that no content rule can see.
  Never commit with `LEFTHOOK=0` or `--no-verify` to get past a finding:
  correct the staged change, or allow the value in `.gitleaks.toml` and say why.
- Never compile software or dispatch builds from target devices. Follow
  `nix/device-cache/README.md` for prebuilt downloads and cache-failure policy.
  Runtime TS/JS transpilation remains part of the plugin scripting contract.
- Keep the AYN Odin 2 Portal bootloader unlocked for stock-based custom
  firmware. Installation and recovery tools must verify the unlocked state and
  stop if the device is locked. They must not contain or run a bootloader lock
  or relock operation. The unlocked state permits AVB key replacement and stock
  rollback if a private signing key is lost. Any bootloader state change is a
  separate destructive operation that needs explicit user approval. Document
  the orange startup warning and the reduced protection from physical access.
- Never put the AYN Odin 2 Portal in a state that needs the case opened to
  recover. The only recovery that needs the case opened is EDL through test
  points, and the only way to reach it is to break the early boot chain. So
  no tool, script, or procedure may write `abl_a`, `abl_b`, `loader_a`,
  `loader_b`, `xbl_*`, `xbl_config_*`, `tz_*`, `hyp_*`, `aop_*`, `devcfg_*`,
  `uefi*`, or any other firmware partition. AYN U-Boot in `loader_a` starts
  systemd-boot from the SD-card ESP. NixOS work produces only SD-card files:
  the ESP, EFI-stub kernel, initrd, DTB, and ext4 root. Prefer tethered
  `fastboot boot <image>` or an SD-card root for first boots so internal UFS
  is untouched. A write to `sda18`/`sda19`
  (ROCKNIX/STORAGE) is allowed only with explicit user approval per run and
  never by an automated path.
- Services are Rust. Wire types live in Rust and are exported through
  Typeshare into `contracts/generated/` — those files are read-only;
  regenerate them via `nix run .#korrid-check`, never edit by hand.
- A surface is a deployable, not a theme. It receives one `SurfaceModel` and one
  `SurfaceHost` through `contracts/surface/` and may import nothing else from
  Korri — no korrid client, no generated Rust types, no host state.
  The host owns facts, effects, and input delivery; the surface owns every
  pixel, including how it presents data Korri does not have. Keep the treaty
  small enough that a surface could ship from another repository unchanged.
- Surfaces are focus-driven. They render native focusable controls and react to
  focus; translating devices into directional movement and confirmation is the
  host's job (`clients/portal/src/input/`), never the surface's.
- The portal's brain is always the korrid on its own device at
  `http://127.0.0.1:<port>`; the portal never talks to another device's korrid
  or any other backend directly. The port and capability arrive in
  `/runtime.json`, which the portal reads before it mounts.
- Plugins are TypeScript or JavaScript **source**, transpiled and evaluated by
  korrid at runtime — never compiled ahead of time, never shipped as native
  code. A plugin returns a declaration and performs no effects; korrid acts on
  it. The sandbox is empty by construction, which is what keeps a plugin
  portable across every device. See `services/korrid/SCRIPTING.md`. The shell
  does not call it yet: wiring waits for a real consumer, because the capability
  model it would declare against does not exist.
- Peer korrids speak the native tagged `/rpc` wire. The Effect-RPC envelope
  client in `services/korrid/src/upstream.rs` exists only for aka until it is
  migrated; it dies at switchover. Do not grow abstractions around it.

# `korri` is the person-facing CLI, and it mirrors the portal

**Owner decision, 2026-10-23.** Initially in design; no command existed yet.

Implementation status: `clients/cli/` now supplies the person CLI and the native
wire uses library folders and leave/resume/end. This is not full acceptance:
safe offline identity reset still lacks exclusive maintenance authority and
service-owned execution. `korri identity reset` refuses without state access;
the old offline reset remains hidden operator tooling, not a safe replacement.
One-off cutover confirmation is still false, so migration and offline owner/grant
helpers remain. Remote session/plugin policy widening is separate work, not
part of this cut. See `clients/cli/README.md` for current behavior and limits.

Today the only command line is the `korrid` daemon binary with `catalog`,
`identity`, `plugin-run` and `plugin-launch` added one slice at a time. Each
has its own argument rules, output format and way to find state. Any unknown
word, including `--help`, starts the daemon.

## Decision

- `korri` is the command a person types. `korrid` stays the daemon. This is
  the `tailscale` and `tailscaled` split.
- The user is the device owner who is comfortable in a terminal. Words come
  from the product, not from the code.
- `korri` mirrors the portal. It uses the same nouns and the same structure,
  and everything a person can change in the portal they can also change with
  `korri`. Commands are noun then verb: `korri <thing> <action>`.
- `korri` talks to the running korrid on its own device by default. Commands
  that need the daemon stopped exist only for explicit recovery and say so.
- Output is for people by default. Every command takes `--json`, and that
  shape is a contract for scripts and agents.
- Internal plumbing (`plugin-launch`, `plugin-prepare`, probes) stays as hidden
  subcommands of `korrid`. It never appears in `korri`'s help.
- When a portal label and a `CONTEXT.md` term disagree, one word is chosen,
  and the portal, `korri` and the glossary all adopt it.
- The words bind `korri`, the surface treaty, and the copy Korri itself
  writes, such as the host's settings labels. A surface owns its own text, so
  for surfaces the words are guidance. `contracts/surface/` points surface
  authors to the glossary and does not copy it.
- `--json` prints the RPC wire types as they are. The wire types are renamed
  to the product words, so there is one schema. A breaking wire change is
  also a breaking change for scripts.
- The feel to match is `gh` and `tailscale`.
- Only the device's person account and root may use `korri` on a device.
  Shell access already means full trust (ADR 0029).
- `korri` reaches korrid through its own socket. inputd's control socket
  stays as narrow as it is.
- A local `korri` call has the portal's authority, as its own entry in the
  policy table in `authorization.rs`.
- `korri` asks for missing input only on a terminal, never when piped.
  Destructive actions ask for confirmation, and `--yes` skips it. Richer
  pickers in the style of Charm's tools follow later.
- `--device <name>` sends a call through the local korrid to a peer. `korri`
  itself still talks only to its own korrid, as the portal does. The peer
  policy table widens so that a device of the same owner can also start
  sessions and manage plugins. Each widening is its own security change with
  its own review.
- The top level has `status`, `version`, `completion`, and the live-session
  verbs `play`, `leave`, `resume` and `end`, beside the mirrored areas.
  Getting games from providers is `korri get`, with `korri downloads` for
  progress.
- `korri devices` covers every device the owner has, this one included:
  `list` marks this device, and `rename [<device>] <name>` renames one. Run
  setup again is `korri setup`. The SteamGridDB key is `korri metadata key`
  while korrid holds it. If ADR 0032 makes it a plugin secret, it moves to
  `korri plugins` with the other plugin settings.
- Launch settings are `korri launch-settings set <key>=<value>`. The
  `--system`, `--game`, `--release` and `--profile` flags choose the layer,
  and the Device layer is the default. `--runner` and `--family` choose the
  runner scope, and any runner is the default. One launch takes
  `korri play <game> --set <key>=<value>`. `show` prints each key's
  effective value and the layer it comes from. Keys follow the wire:
  `cpu.min`, `cpu.max`, `gpu.min`, `gpu.max`, runner setting names, and
  `<modifier>.enabled` and `<modifier>.<setting>`. Hooks, environment and
  arguments stay read-only, as in the portal (ADR 0029).
- `--device <name>` takes a device name without case, or a device key. An
  unclear name lists the matches and stops. It never guesses.
- An error is one line with the cause in product words, then one line with
  the next step. Exit 1 is a failure, 2 is bad usage, and 4 means korrid
  cannot be reached. With `--json`, errors go to stderr as JSON.
- Sleep, power off and Restart Korri wait until korrid has calls for them,
  so the portal and `korri` share one path.
- `korri` is Rust, in `clients/cli/`.
- The old `korrid catalog` and `korrid identity` commands go in one cut,
  after `korri` covers them. There is no alias.
- In that cut, folder pairing, unpairing and rescans become live `korri`
  commands. `reset` becomes a `korri` recovery action. `identity status`,
  `owner-binding-request` and `identity import` stay as hidden `korrid`
  subcommands, because boot runs them before korrid starts. Ownership moves
  only through `korri identity switch`, the same identity switch as the
  portal.
  `catalog migrate`, `replace-test-owner-offline` and
  `reconcile-stale-grants-offline` are deleted after each device is known to
  have run them.
- Words: **End** destroys a live session and **Resume** goes back to one
  (the glossary's Return is renamed). **seat** names a controller seat.
  **saves** names the area for save history and snapshots. **Library
  folder** replaces the glossary's Source.
- Remote widening stops at sessions and plugins. Wi-Fi, saves, identity and
  onboarding stay local to the device.
- `korri play <game>` matches titles without case. One match plays. Several
  matches give a picker on a terminal, or an error that lists them when
  piped. An exact game ID always works.
- `korri status` shows korrid's state and version, the device name and
  owner, the live session, nearby devices and whether each can be reached,
  the count of attention items, the Wi-Fi network and address, and free
  storage.

## Considered options

- Keep everything under `korrid`: no new binary, but the daemon and the tool
  share one argument space, which is how `--help` came to start the daemon.
- Offline only: no new permission surface, but the CLI could never show live
  state, and every change would need the daemon stopped.
- A curated subset or a recovery-only complement of the portal: less to
  build, but the person has to guess what the terminal can do.

## Consequences

- A new setting in the portal needs a `korri` command in the same change.
- `korri` needs an authorised local channel to korrid for the person's
  account. That is a permission decision of its own.

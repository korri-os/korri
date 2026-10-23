# `korri` is the person-facing CLI, and it mirrors the portal

**Owner decision, 2026-10-23.** In design; no command exists yet.

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
- The feel to match is `gh` and `tailscale`.

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

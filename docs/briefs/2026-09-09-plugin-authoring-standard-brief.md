---
date: 2026-09-09
topic: plugin-authoring-standard
artifact: brief
status: design decided, not implemented
supersedes: the 2026-09-09 first draft of this file (commit c40480e6)
---

# Plugin authoring standard

The design below has user approval, decision by decision. Nothing in it is
implemented. Field names and file layouts are illustrative unless a section
says a real producer already exists. Implementation must ground each schema
in a real producer, consumer or explicit user choice, as `AGENTS.md`
requires.

Worked examples live on `aso:~/korri-review/scenario-full/` (33 files). That
tree predates the gap decisions in this file. Where the two disagree, this
file wins. The `aso` tree is scratch; this file is the durable record.

## Shipped

These changes exist and are pushed.

- `korri-os/plugins` publishes standard signed Nix caches through GitHub
  Releases. Payload NARs live in immutable batch releases. Signed `.narinfo`
  files live in one public mutable `cache` release. See `PUBLICATION.md` in
  that repository. Repository main: `74a9d8c`.
- The publisher omits dependencies already verified in trusted upstream
  caches. The live Tailscale publication is five hosted files: two plugin
  payloads (1,288 compressed bytes) and three metadata files. Dependencies
  come from `cache.nixos.org`.
- Core's plugin installer realizes exact store outputs across the plugin
  cache and configured trusted substituters. Local, remote and fallback
  builds stay disabled. Core main: `a17c5c35`.
- `AGENTS.md` has a "Plugin boundaries" section (`d81b3077`).
- The RG353M is named `haku` (`e7c326f5`). `nixosConfigurations.haku` and
  `haku-rescue` exist on branch `feat/haku-plugin-host` in worktree
  `.worktrees/haku-plugin-host`. Not merged, not built, not deployed. The
  device runs generation 9 from eMMC at `192.168.1.239`, hostname still
  `rg353m`.

Not done: Tailscale installation on haku, host DNS integration, migration of
any plugin to the layout below, and every host change listed at the end.

## Problem

The current Tailscale `plugin.ts` in `korri-os/plugins` contains systemd
service directives that the host translates back into a service file. The
user rejected TypeScript as a second configuration language for systemd.
Legacy already split identity (`src/plugin.ts`) from packages and services
(`nix/composition.nix`, `nix/nixos-module.nix`). The standard restores that
split and extends it to launchers, runtimes, dependencies and versions.

## Layout

Two files per plugin.

```text
plugins/<name>/
├── plugin.nix   Nix: packages, named files, services, required plugins
└── plugin.ts    TypeScript: identity, Korri contributions, launch callback
```

`plugin.nix` is evaluated in CI only. It returns:

| Attribute | Meaning |
|---|---|
| `packages.<name>` | A derivation the plugin ships. |
| `files.<name>` | A path inside a package that `plugin.ts` names by key. The builder runs `test -e` on each; a nixpkgs layout change fails in CI. |
| `services.<name>` | Standard NixOS `systemd.services` options. CI renders the unit through `eval-config.nix`. No Korri template language. |
| `requires` | Exact plugin store paths, resolved from the repository's plugin set or a flake input. Nix carries them in the closure. |

`plugin.ts` uses named exports. The sandbox stays empty. Exports:

| Export | Meaning |
|---|---|
| `name`, `title`, `description` | Identity. No `namespace` export: the publisher is signed in at publish time. |
| `systems` | Systems this plugin declares. |
| `services` | Keys of `services` in `plugin.nix` that this plugin activates. |
| `launchers` | Launcher kinds and launcher instances. See the model below. |
| `runtimes` | Cores. Each names one launcher. |
| `discovery` | Data only. The `fileReleases` record as shipped today in `plugins/mgba/plugin.ts`. No callback. |
| `android` | An intent reference for platforms with no daemon. |

`plugin.nix` means "there is a Linux payload built by Nix". It does not mean
NixOS. Ubuntu with Nix installed runs the same host binary and the same unit.
Android reads `android` and ignores the rest.

## Identity

Every ID is `@publisher:plugin/local`. The publisher comes from the signing
key of the repository that published the build, written into the generated
manifest at publish time. Nothing inside a plugin file can claim one. IDs
never carry a version.

Three rows:

| Thing | What it owns | Example |
|---|---|---|
| Launcher kind | `launch`, the typed settings table, `reservedKeys`, and the default build | `@korri:retroarch/retroarch` |
| Launcher instance | `{ kind, program }`: a specific build of a kind, shipped by any plugin | `@korri:snes9x/retroarch` |
| Runtime | A core. Names one launcher instance. | `@simon:mgba-dev/mgba` |

"Which RetroArch" is always a launcher ID. The kind's own launcher is also
an instance. A core that needs a specific RetroArch build declares its own
instance and ships that build in `plugin.nix`.

## Configuration cascade

Legacy's layer model, with launcher and runtime IDs fully qualified. Layers,
least to most specific: launcher kind, launcher instance, system, runtime,
game, override. Top-level records are `launchers`, `systems`, `runtimes`,
`games`. Each record carries a `launchers.<full-id>` block for its
launcher-specific settings. No key names a resolver step; legacy's plans
named `byLauncher` as debt and this design does not carry it.

Typed settings are validated against the launching build. Raw `extraConfig`
passes through after Korri's lines and the typed lines; RetroArch decides,
and Korri records which build ran. `reservedKeys` blocks legacy's credential
keys plus `kiosk_mode_enable`, `config_save_on_exit` and `menu_driver`.

Two choices the assistant made and marked, still open to reversal:

- A kind's config folds into its instances before the instance's own config.
  Legacy configured each app on its own.
- A typed setting the launching build cannot honor is omitted with a visible
  message, and the launch proceeds.

## Versions

| Version of | Identity | Source | Used for |
|---|---|---|---|
| The plugin build | store path | Nix, by content | install, approval, closure, `requires` |
| The plugin release | publishing commit, batch tag `build-<rev12>`, label `date · shortrev` | CI | update, rollback, "which one is on haku" |
| The program inside | the pinned package's version | Nix | display only; validation uses the source, see gap 8 |

No version ranges anywhere. A device pins by exact store path in its
selection record. A plugin pins a dependency through `flake.lock`. A human
names a release by commit: `korri-plugin install CACHE ID --release <commit>`
resolves the batch release, shows the store path, and the user approves that
path. No publisher pointer files; add them only when a publisher needs to
retract a build.

## Permissions

The rendered unit is the request. The host validates it against a directive
allowlist and layers its own hardening as a drop-in that always wins. The
allowlist today, from `unit.rs`: `Type`, `ExecStart`, `ExecStopPost`,
`CapabilityBoundingSet` limited to `CAP_NET_ADMIN` and `CAP_NET_RAW`.
Tailscale adds `DeviceAllow=/dev/net/tun rw` and
`LoadCredential=authkey:tailscale-authkey`. The allowlist grows one directive
per real plugin, and every allowed directive appears in the approval prompt.
A directive not on the list is refused by name, never dropped.

One Korri-owned field exists because no unit directive carries it: `ports`.
The host applies it; the plugin never touches the firewall.

Namespace binding: the device maps each publisher name to one signing key
and one cache URL. After Nix verifies the download, the host reads the
namespace from the manifest and refuses the install if the key that signed
the NAR is not the key bound to that namespace.

NixOS modules render in CI only. A plugin never ships a module the device
evaluates. Evaluating an upstream module and extracting only its unit, with
a CI report of every other option the module set and the builder dropped, is
supported later, when a second daemon plugin needs it.

## Firewall on haku (verified 2026-09-09, read-only)

| Fact | Value |
|---|---|
| Unit | `firewall.service`, NixOS `firewall-iptables.nix` scripts |
| Tool | `iptables v1.8.11 (nf_tables)`; `nft` is not installed |
| Kernel | `nf_tables` and `nft_compat` modules loaded |
| `INPUT` | policy ACCEPT, one rule `-j nixos-fw` |

The host creates one chain it owns, `korri-plugins`, and inserts
`-I INPUT 1 -j korri-plugins` ahead of `nixos-fw`. Port rules go in that
chain. The NixOS start, reload and stop scripts at the pinned nixpkgs
revision flush and rebuild only the `nixos-fw*` chains and never flush
`INPUT`, so the chain survives a `firewall.service` reload and a
`nixos-rebuild switch`. It does not survive a reboot; the host re-applies it
with `After=firewall.service`. Ubuntu needs one backend detection at the
edge.

## Decisions

Binding. Do not reopen without new evidence.

| # | Question | Decision | Cost accepted |
|---|---|---|---|
| 1 | Nothing binds a namespace to a signing key. | Add the publisher binding now. | Three lines per publisher on the device instead of two. |
| 2 | Plugins can only ask for a unit. | Permissions are allowlisted systemd directives plus one `ports` field. No new Korri vocabulary. | The approval prompt shows raw directives. |
| 3 | `disable`, `remove` or `update` can leave a runtime pointing at a missing launcher or kind. | Refuse. The host re-resolves every installed runtime's `launcher` and every instance's `kind` against the post-operation state and refuses by name. | A launcher export rename needs `remove` then `install`. |
| 4 | Discovery callbacks contradict `SCRIPTING.md` and the scanner's 100,000-entry budget. | Discovery is data: the shipped `fileReleases` record. `ClaimConflict` stops being terminal; a file with several claims is one candidate and the route resolver offers a chooser. | One scanner change. |
| 5 | `launch` output is unvalidated. | No validation beyond approval. The approval digest covers the `plugin.ts` bytes, so the administrator approves the exact `launch` function. The existing `systemd-run` sandbox stays. | A bad plugin update reaches the runtime user's files and display sockets with no check in between. |
| 6 | Two runtimes share one flat savestate directory. | The RetroArch kind writes `savestate_directory = states/<runtime-id>/`. Saves, BIOS and screenshots stay shared. Host unchanged. | Each launcher kind decides portability for its own program. |
| 7 | The host keeps only the active build; no rollback exists. | Keep current and previous. `update` moves `active` to `previous`; `korri-plugin restore ID` swaps them with no download. Two stored approvals per plugin. | At most two builds per plugin on disk. |
| 8 | The `since` table is hand-written. | Drop `since`. The table holds `key` and `type`. CI checks each key against `configuration.c` of the instance's pinned RetroArch (802 registrations at 1.22.2), type included. The check lives with the kind, not in the shared builder. | Launcher kinds that are not RetroArch supply their own check or none. |
| 9 | No plan to retire a signing key. | New key only. Republish the builds worth keeping under new batch tags. One `--key-file`, one key per publisher. | Builds not republished become uninstallable once devices drop the old key. |
| 10 | Route choices are per device. | Deferred by the `AGENTS.md` capability guard. When a stored choice names a runtime that is not installed, the resolver shows the chooser among installed runtimes and keeps the stored choice. | The same config yields different routes on different devices, visibly. |

Earlier decisions that stand: devices never compile; GitHub Releases as the
signed cache with dependencies from `cache.nixos.org`; the signing key is the
trust anchor, not tags; manifests are generated; Nix owns programs and
services, TypeScript owns identity and callbacks; callbacks only where a
runtime input exists; named exports; standard NixOS service options rendered
in CI.

## Corrections recorded

- The source session said the host would run `nft add rule ...`. Wrong for
  haku: `nft` is absent. The tool is `iptables -w` and `ip6tables -w`.
- The `aso` version example shows `korri-plugin restore` returning to an
  older release. That command does not exist today; gap 7 creates it.
- The gap list said RetroArch keys savestates by core name. Under Korri's
  generated config the layout is flat by ROM name, shared by every core.
  Battery saves are meant to be shared; only savestates collide.

## Host changes this design needs

Grouped by crate. File references are to today's main.

| File | Today | Target |
|---|---|---|
| `services/korrid/src/script.rs` | Script evaluation, completion value, functions rejected | ES module evaluation, export names checked against a contract, data exports as JSON, function exports callable under the same budgets |
| `plugin-host/src/unit.rs` | Renders a unit from `contributes.daemons` | Loads the shipped unit, validates directives against the allowlist, writes the host drop-in |
| `plugin-host/src/host.rs` | One `selection.json` per identity, `active` and `pending` roots | Reads the manifest; walks `requires` before install; refuses lifecycle changes that break a dependent (gap 3); keeps `previous` and implements `restore ID` (gap 7); applies `ports` through the `korri-plugins` chain |
| `plugin-host/src/package.rs` | Realizes one path across caches | Unchanged for fetch; adds the namespace-to-key check after verification (gap 1) |
| `services/korrid/src/discovery/scanner.rs` | `ClaimConflict` drops the file | Several claims become one candidate (gap 4) |
| `services/korrid/src/config/resolver.rs` | Two runtimes for one system is `RouteUnavailable` | Returns candidates; stored choice per game or system by full runtime ID; missing runtime falls back to the chooser (gap 10); instance resolves to its kind |
| `services/korrid/src/launcher/linux_retroarch.rs` | Hard-codes RetroArch and three `KORRI_*` env vars | A generic Linux launch runner: fold the cascade, validate typed settings against the kind's table, call `launch`, write files, spawn through the existing `systemd-run` path |
| `services/korrid/package.nix` | Bundles four `plugins/*.plugin.ts` and sets the env vars | Both go when RetroArch and mGBA move to the plugin host. One-way cut. |

CI, in `korri-os/plugins`: the shared builder renders units, checks `files`,
checks every cross-plugin reference against the pinned build, and runs the
RetroArch kind's key check (gap 8). `korri-cache export` stays at one key.

## Order of work

1. Tailscale on the new layout: `plugin.nix` with standard service options,
   thin `plugin.ts`, host loads the rendered unit, namespace binding, `ports`
   through the `korri-plugins` chain. VM test. Publish. Install on haku.
2. Land `feat/haku-plugin-host` after rebasing onto current main; add the
   `korri-plugins-1` key, the cache URL and the publisher binding to the
   haku generation.
3. RetroArch and mGBA move out of korrid into plugin packages. Module
   evaluation in `script.rs`, the generic launch runner, the scanner and
   resolver changes. This touches the GBA path that works on haku today.
4. `previous` root and `restore ID`. Dependency refusal on lifecycle
   commands.
5. A second launcher kind, or a second daemon plugin, is the trigger for
   upstream-module extraction and for any change to the state layout.

## Where to read

- `korri-os/plugins`: `README.md`, `PUBLICATION.md`, `nix/github-cache.py`.
- Core: `services/korrid/plugin-host/README.md`, `src/host.rs`,
  `src/unit.rs`, `src/package.rs`, `vm-test.nix`;
  `services/korrid/SCRIPTING.md`, `src/script.rs`,
  `src/discovery/scanner.rs`, `src/host/systemd_unit.rs`.
- Legacy reference, read-only: `product/plugins/AGENTS.md`,
  `product/platform/plugin/index.ts`,
  `product/plugins/retroarch/src/launch-spec.ts`,
  `product/platform/library/config/cascade-resolver.ts`,
  `docs/plans/2026-06-05-001-feat-readable-library-schema-plan.md`,
  `docs/deployment/korri-launch-config.md`.
- Examples: `aso:~/korri-review/scenario-full/`, to be corrected against
  this file.
- Probe scripts: `/tmp/korri-launch/probe-haku.sh`,
  `/tmp/korri-launch/probe-haku-firewall.sh`.

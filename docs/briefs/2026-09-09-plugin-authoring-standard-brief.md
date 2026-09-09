---
date: 2026-09-09
topic: plugin-authoring-standard
artifact: brief
status: design discussion, not implemented
---

# Plugin authoring standard

Design handoff for the next session. The direction below has user approval. The
file layouts and field names are illustrative unless a section says otherwise.
Implementation must ground each schema in a real producer, consumer or explicit
user choice, as required by `AGENTS.md`.

## Shipped this session

These changes exist and are pushed.

- `korri-os/plugins` publishes standard signed Nix caches through GitHub
  Releases. Payload NARs live in immutable batch releases. Signed `.narinfo`
  files live in one public mutable `cache` release. See
  `PUBLICATION.md` in that repository. Repository main: `74a9d8c`.
- The publisher omits dependencies already verified in trusted upstream caches.
  The live Tailscale publication is five hosted files: two plugin payloads
  (1,288 compressed bytes) and three metadata files. Dependencies come from
  `cache.nixos.org`.
- Core's plugin installer realizes exact store outputs across the plugin cache
  and configured trusted substituters. Local, remote and fallback builds stay
  disabled. Core main: `a17c5c35`.
- `AGENTS.md` has a "Plugin boundaries" section (`d81b3077`). It requires thin
  declarations, native configuration in native formats, generated metadata and
  unchanged host validation.
- The RG353M is named `haku` (`e7c326f5`). `nixosConfigurations.haku` and
  `haku-rescue` exist on branch `feat/haku-plugin-host` in worktree
  `.worktrees/haku-plugin-host`. That branch also enables the plugin host on
  the device. It is not merged, not built, and not deployed. The device
  currently runs generation 9 from eMMC at `192.168.1.239`.

Not done: Tailscale installation on haku, host DNS integration, and any
migration of existing plugins to the new layout.

## Problem

The current Tailscale `plugin.ts` in `korri-os/plugins` contains systemd
service directives (`Type`, `ExecStart`, `ExecStopPost`,
`CapabilityBoundingSet`). The host translates them back into a service file.
The user rejected this: TypeScript must not become a second configuration
language for systemd. Legacy already followed the intended split. Legacy
`src/plugin.ts` held identity and Korri records; `nix/composition.nix` and
`nix/nixos-module.nix` held packages and services. The current Tailscale
declaration broke that pattern.

## Approved layout

Two files per plugin.

```text
plugins/<name>/
├── plugin.nix   Nix: programs, services, build-time outputs
└── plugin.ts    TypeScript: identity, Korri contributions, runtime callbacks
```

Rules:

- `plugin.nix` is evaluated in CI only. Devices receive outputs. Devices never
  evaluate Nix or compile.
- Services are written as standard NixOS `systemd.services` options. CI renders
  the `.service` file. No hand-written service templates, no placeholder
  substitution, no service directives in TypeScript.
- `plugin.ts` references Nix outputs by name. CI resolves names to exact store
  paths.
- Runtime behavior uses callbacks that return declarations. Korri performs the
  effects. Prefer callbacks over template strings such as `{content.path}`;
  one mechanism, not two.
- The host still validates every artifact and requires administrator approval.
  A native service file does not bypass host policy.

## Illustrative examples

### Tailscale, Linux

`plugin.nix`:

```nix
{ pkgs }:
{
  packages.tailscale = pkgs.tailscale;
  services.tailscaled = {
    serviceConfig = {
      Type = "notify";
      ExecStart = "${pkgs.tailscale}/bin/tailscaled --state=\${STATE_DIRECTORY}/tailscaled.state --socket=\${RUNTIME_DIRECTORY}/tailscaled.sock --port=41641";
      ExecStopPost = "${pkgs.tailscale}/bin/tailscaled --cleanup";
      CapabilityBoundingSet = [ "CAP_NET_ADMIN" "CAP_NET_RAW" ];
    };
  };
}
```

`plugin.ts`:

```ts
({
  namespace: "@korri",
  name: "tailscale",
  title: "Tailscale",
  services: ["tailscaled"],
})
```

### mGBA, Linux

Today korrid finds the core through `KORRI_MGBA_CORE`, set at korrid build
time in `services/korrid/package.nix`. The target is that the plugin carries
the core and korrid reads the resolved path from the installed plugin.

`plugin.nix`:

```nix
{ pkgs }:
{
  packages.core = pkgs.libretro.mgba;
}
```

`plugin.ts`:

```ts
({
  namespace: "@korri",
  name: "mgba",
  title: "mGBA",
  systems: { gba: { title: "Game Boy Advance" } },
  discovery: ({ file }) =>
    file.extension === ".gba"
      ? { system: "gba", launcher: "@korri:retroarch/retroarch", runtime: "mgba" }
      : null,
  runtimes: {
    mgba: {
      kind: "libretro-core",
      launcher: "@korri:retroarch/retroarch",
      systems: ["gba"],
      resolve: ({ packages }) => ({
        core: packages.core.path("lib/retroarch/cores/mgba_libretro.so"),
      }),
    },
  },
})
```

Android is out of scope for this pass.

## Decisions recorded

| Question | Decision |
|---|---|
| Authoring language | Both, with fixed roles. Nix owns build and services. TypeScript owns Korri integration and runtime callbacks. |
| Callbacks or templates | Callbacks. Template substitution is not part of the launcher schema. |
| Service definitions | Standard NixOS module options, rendered in CI. |
| Manifests | Generated from source at publish time, never hand-maintained. |
| Distribution | GitHub Releases as a signed Nix cache, dependencies from `cache.nixos.org`. |
| Device builds | Never. `max-jobs = 0`, `builders = ""`, `require-sigs = true`. |
| Tags | Batch tags such as `build-<commit>`, not plugin version tags. |
| Immutability | Payload releases immutable. The `cache` metadata release stays mutable. Repository protection is enabled through the REST API. |

## Open items

- The host's `contributes.daemons` contract still expects service directives.
  It must change to accept a rendered service file plus its validation. This
  changes a security boundary and needs its own tests.
- No `korri.plugin` helper, `packages.<name>.path(...)` accessor, or
  `services: [...]` field exists. Ground them in the first real consumer.
- Discovery and runtime callbacks need a sandbox contract: inputs, output
  shape, bounds. See `services/korrid/SCRIPTING.md`.
- The `KORRI_MGBA_CORE` environment path stays until korrid can read the core
  path from an installed plugin.
- The metadata release holds at most 1,000 files. No rotation exists.
- The `feat/haku-plugin-host` branch must be rebased onto current main before
  it builds; it predates the importer change.

## Where to read

- `korri-os/plugins`: `README.md`, `PUBLICATION.md`, `nix/github-cache.py`.
- Core: `services/korrid/plugin-host/README.md`, `src/package.rs`,
  `src/unit.rs`, `vm-test.nix`.
- Legacy reference, read-only: `product/plugins/AGENTS.md`,
  `product/plugins/melonds/`, `product/plugins/retroarch/`.
- Session evidence: `/tmp/korri-launch/`, including
  `lean-publication-verified.json` and `upstream-cache-verified.json`.

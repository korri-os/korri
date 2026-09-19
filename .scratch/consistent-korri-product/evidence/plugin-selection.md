# Evidence for the opinionated plugin selection

Decision: [Choose the opinionated plugin selection](../issues/03-choose-default-plugins.md)

## Scope

Current core source was inspected at `a17cad1f`; later edits in this worktree only record the ongoing discussion. The separate local checkout `/home/simonwjackson/code/sandbox/korri-plugins` was clean at `7f4eae0c27750118ec6c39663f8644145cc7bdd8`. Legacy references use core commit `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62`.

Two read-only scouts inventoried current outputs and historical plugin families. The parent checked the package composition, selected catalogue entries, generated runner declaration, and both PPSSPP implementations. A source-count script found 90 catalogue entries and 62 distinct declared system identifiers. This is not a build, publication, compatibility, performance, or device-acceptance result. No network release assets or installed receipts were inspected.

## Current source inventory

Paths beginning `korri-plugins:` belong to the separate repository at the pinned commit above.

| Category | Evidence | What it establishes |
|---|---|---|
| Emulator catalogue | `korri-plugins:plugins/libretro/cores.nix`; `plugins/libretro/default.nix`. | Ninety core entries generate separate plugin outputs. They include overlapping choices for NES, SNES, GBA, N64, PlayStation, and other systems. Declared coverage is not verified runtime support. |
| Shared frontend and settings | `korri-plugins:plugins/libretro/core-plugin.nix`, lines 19-84 and 113-135; `plugins/retroarch/plugin.ts`. | Each generated runner carries its frontend and core. Identical Nix paths can deduplicate. The separate RetroArch plugin supplies a settings family, not another runner. Removing that settings-family plugin must not be confused with removing every runner's frontend dependency. |
| Current generated controls | `korri-plugins:plugins/libretro/core-plugin.nix`, lines 59-79. | Generated declarations expose Open RetroArch menu and Quit game. They do not establish complete integrated save-state and fast-forward behavior. |
| Architecture constraints | `korri-plugins:nix/default.nix`; `plugins/libretro/cores.nix`, lines 12-25. | Outputs target x86_64-linux and aarch64-linux. ParaLLEl N64 retains its ARM exclusion after a recorded link failure. PPSSPP's exclusion is lifted specifically in this catalogue. Other platform restrictions remain; no full build matrix was verified here. |
| Tailscale | `korri-plugins:plugins/tailscale/plugin.ts` and `plugin.nix`. | A real service plugin exists. Login is an explicit action; installing or enabling the daemon does not join a tailnet. Network privileges and TUN support are separate requirements. |
| SSH | [Current declaration](../../../plugins/ssh/plugin.ts), [native policy](../../../plugins/ssh/sshd_config), and [README](../../../plugins/ssh/README.md). | Optional key-only device administration exists. It is not owner enrollment and does not replace the agreed self-service product requirement. |
| Output boundary | `korri-plugins:nix/default.nix`, lines 50-58. | The source exposes the 90 core packages, RetroArch settings, Tailscale, and SSH. Host/cache tools are not plugins. This output set contains no acquisition provider or streaming plugin. |
| Publication evidence | `korri-plugins:PUBLICATION.md`; `.github/workflows/plugin-repository.yml`; `README.md`. | The workflow's default selection is Tailscale, RetroArch, mGBA, and SSH, not the full catalogue. Source documentation records a cold-host VM gate, not physical acceptance. Source presence and release tooling do not prove a usable published package. |

## A selection conflict to resolve explicitly

Medium importance: core's [standalone PPSSPP](../../../plugins/ppsspp/plugin.ts) and `korri-plugins:plugins/libretro/cores.nix` both generate plugin identity `@korri:ppsspp` and runner identity `@korri:ppsspp/ppsspp`. The generated plugin's identity rules are visible in `plugins/libretro/core-plugin.nix`.

These are different implementations. Standalone PPSSPP declares `iso`, `cso`, and `pbp` discovery and currently refuses authored settings and raw overrides. The generated libretro version declares `prx`, `pbp`, and `chd` discovery and uses the shared RetroArch helper and controls. Selecting PSP therefore requires an explicit implementation/source choice. Do not count the two as independent defaults or claim either has passed the required device tests. No rename or packaging change is authorized by this observation.

## Legacy families outside retro emulation

Read these with `git show 0e4cec9:<path>`. Their existence does not select a default or authorize restoring old schemas.

| Family | Historical primary sources | Selection limit |
|---|---|---|
| Streaming | `product/plugins/moonlight/src/plugin.ts`. | Concrete launch and stream-control handlers exist. A current independent Moonlight plugin output was not found in the inspected trees. Current Sunshine service composition is a separate integration. |
| Content providers | `product/plugins/itchio/src/definition.ts`; `smwcentral/src/plugin.ts`; `levelsharesquare/src/plugin.ts`; `community-catalog/src/plugin.ts`. | Real providers and curated listings exist, with different acquisition responsibilities. Their current upstream operation was not tested. |
| Ports and native applications | `product/plugins/portmaster/src/plugin.ts`; `gmloader/src/plugin.ts`; `neverball/index.ts`; `steam/src/plugin.ts`. | Port installation, native play, and Steam have historical implementations of different scope. These are not automatically valid packages under current plugin contracts. |
| Compatibility runtimes and session integrations | `product/plugins/box64-runtime/src/plugin.ts`; `fex-runtime/src/plugin.ts`; `proton-runtime/src/plugin.ts`; `gamescope/src/plugin.ts`; `remap/index.ts`; `turnip/src/plugin.ts`. | Some integrations depend on other software or hardware. They are not a universal bundle to enable on every device. |
| Web content | `product/plugins/webpage/index.ts`; `web-canvas/index.ts`. | Historical web-runner declarations and helpers exist. Individual game listings are distinct from reusable runners. |
| Fixtures and incomplete listings | `product/plugins/acquisition-fixtures/index.ts`; `shipwright/index.ts`; [current PPSSPP README](../../../plugins/ppsspp/README.md). | Canned provider results and source/acquisition metadata must not be counted as complete playable integrations. The standalone PPSSPP proof is real code but has stated settings/control gaps. |

## Existing device selections are not the default bundle

[RP Mini V2 integration](../../../nix/devices/rpminiv2/game-plugins.nix) configures trust, receipt restoration, and access for 13 library slugs. It does not choose a named default runner list. Neither those directories nor this repository inventory establishes what is installed on an actual device.

The live ticket owns all bundle decisions. Exact runners, included systems, additional services/providers, and installation/login choices remain open. Size measurement and the minimal-versus-opinionated image decision remain later work.

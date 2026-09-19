# Product behavior inventory for the intended-product decision

Decision: [Define the intended Korri product](../issues/01-define-intended-product.md)

## Scope and confidence

This is evidence for a live product discussion, not an approved feature list. Current source was inspected at `4ce4dda9`. Legacy evidence uses `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62`. Three read-only scouts covered current consumers, legacy behavior, and prior decisions. The parent checked the primary sources cited below for the main findings.

No builds, runtime tests, live downloads, or device acceptance checks were performed. A wired caller establishes a code path, not a working device experience. A backend operation is not proof that a user can reach it. A legacy feature is not automatically a current requirement.

## Current product consumers

| Behavior | Source evidence | Limit or missing connection |
|---|---|---|
| Start the product. | [Portal composition](../../../clients/portal/src/main.tsx), lines 64-101, mounts the catalog using the local Linux RPC binding. | This entrypoint has no onboarding branch. Development uses an in-memory client. |
| Set device identity and ownership. | [Settings](../../../clients/portal/src/surface/settings-model.ts), lines 57-75, can change the device name. [Identity CLI](../../../services/korrid/src/identity_cli.rs), lines 12-85, supports owner-binding request/import, status, reset, and restricted repair. | Naming is not enrollment. No owner enrollment consumer was identified in the inspected portal composition or surface host contract. |
| Browse a library and select where to play. | [Catalog loading](../../../clients/portal/src/surface/use-launchables.ts), lines 145-180, reads catalog, local games, session, health, settings, and discovery. [Surface contract](../../../contracts/surface/korri-surface.ts) exposes launch locations and requires a choice when more than one exists. | A visible game is not proof of a runnable route. This is not proof of the general capability matcher described in repository intent. |
| Discover local games and enrich metadata. | [Settings](../../../clients/portal/src/surface/settings-model.ts), lines 76-148, expose a SteamGridDB key, folder scan status, rescan, and removal. [Portal effects](../../../clients/portal/src/surface/use-launchables.ts) call korrid for those actions. | The scout found a folder-registration client operation but no production caller. The inspected surface host contract offers no acquisition command. Neither observation proves these operations are absent from every backend. |
| Choose an installed local runner. | [SurfaceRoot](../../../clients/portal/src/surface/SurfaceRoot.tsx) connects local catalog launches to the [runner chooser](../../../clients/portal/src/surface/runner-chooser.ts). The chooser exposes one-time and remembered choices and calls korrid to launch. | Requires a usable installed route. Hardware play was not tested. |
| Prepare play on another device. | [Launch effects](../../../clients/portal/src/surface/use-launchables.ts), lines 497-526, call `sessionPrepare` with the selected game and device. | The inspected caller does not start a streaming viewer after preparation. Preparation is not end-to-end streamed play. |
| Resume and stop a session. | [Launch effects](../../../clients/portal/src/surface/use-launchables.ts), lines 471-489 and 534 onward, name the exact launch for thaw and stop. Stop completion is observed. | This does not prove durable suspend, reboot resume, or transfer of a running session. |
| Operate live gameplay controls. | [Portal composition](../../../clients/portal/src/main.tsx), lines 50-62, selects the in-memory overlay controller. Its comment explicitly says Linux in-game control is not connected there. | The overlay's controls are fixtures, not evidence of live Linux gameplay control. Backend/plugin controls need separate tracing. |
| Manage plugins. | [Settings](../../../clients/portal/src/surface/settings-model.ts), lines 94-103, expose plugin On/Off choices. | Enablement is not installing, updating, restoring, uninstalling, or reclaiming storage. |
| Set up and maintain the device. | The complete [settings producer](../../../clients/portal/src/surface/settings-model.ts) exposes name, metadata credential, plugin enablement, discovery, counts, and software version. | It does not expose network setup, volume, brightness, power, or recovery controls. Physical controls and other service paths were not exhaustively assessed. |

## Legacy behavior worth discussing

These references can be read with `git show 0e4cec9:<path>`. Exact historical schemas and device roles are not proposed for adoption.

| Behavior | Primary source | What it establishes |
|---|---|---|
| Search and acquire content through providers. | `product/apps/portal/api/acquisition/acquire.rpc-handler.ts`, lines 8-15; `product/platform/acquisition/plugins/registry.ts`, lines 25-47. | The RPC starts an acquisition job with a placement runner and discovery providers. This is actual backend wiring, not proof of a complete handheld acquisition flow. |
| Discover external catalogs and install content. | Scout inspected handlers under `product/plugins/itchio/`, `smwcentral/`, `levelsharesquare/`, `portmaster/`, `community-catalog/`, and `steam/`. | Historical sources go beyond launching files already present. Provider availability, successful acquisition, and continued product intent still require verification. Do not confuse installation of content through a plugin with installation of the plugin itself. |
| Play streams and adjust stream controls. | `product/plugins/moonlight/src/plugin.ts`, lines 55-99. | Concrete handlers compose a stream launch, describe/apply controls, and connect a control session. This is not a current Linux deployment claim. |
| First-use and network setup. | `product/surfaces/web/pico/pages/system/Onboarding.tsx`; `product/surfaces/web/pico/pages/settings/NetworkSettings.tsx`. | Parent inspection confirmed static text and fixed values. These screens do not establish working onboarding or network setup. Copying them would not fill the current behavior gap. |

## Existing intent that must not be rediscovered by guessing

| Area | Recorded agreement | Authority and limit |
|---|---|---|
| Federation and hardware. | Devices have capabilities, not fixed roles. Every device runs korrid; a screen is optional. Content can have local and streamed fulfillment routes. | [AGENTS.md](../../../AGENTS.md), Federation. The capability model is deliberately unbuilt; these principles do not authorize a speculative framework. |
| Plugin lifecycle. | A previously unknown plugin can install, enable, update, and remove without compiling or updating NixOS. Tailscale is optional. | [September 8 installation brief](../../../docs/briefs/2026-09-08-linux-plugin-installation-brief.md), Chosen thing and Goals. Current CLI/VM evidence is not a finished user-facing experience. |
| Plugin sources and trust. | Built-in official HTTPS catalog, user-added catalogs, explicit source selection, and updates tied to that source. Installation approval remains separate from catalog trust. | [September 8 installation brief](../../../docs/briefs/2026-09-08-linux-plugin-installation-brief.md), Plugin repository sources. Signature checks remain required. |
| Plugin participation and sessions. | Plugins can contribute beyond runners. Native versions can coexist; composition order is explicit. Korri owns sessions and cleanup after exit or failed startup. | [September 15 model brief](../../../docs/briefs/2026-09-15-plugin-model-brief.md), Decisions to retain. The operation inventory is proposed coverage, not an approved framework to build in full. |
| User, content, and hardware settings. | Content facts travel with content; hardware facts stay with hardware; opinions travel with each isolated user. Cards and multiple configuration sources are part of the recorded direction. | [September 6 discussion](../../../docs/briefs/2026-09-06-config-cascade-discussion.md), opening intent and Decided. Later runner/family decisions require a conflict review before reusing its concrete schema. |
| Plugin rollback. | Keep current and previous plugin builds and allow an offline restore. | [September 9 authoring brief](../../../docs/briefs/2026-09-09-plugin-authoring-standard-brief.md), Decisions, row 7. This earlier agreement must be reconciled with newer lifecycle decisions rather than treated as if no retention choice was ever made. |
| Saves and savestates. | September 9 chose shared saves/BIOS/screenshots and runtime-separated savestates. September 15 still leaves persistent save/state identity unresolved. | The two plugin briefs above. Storage layout, portability, and identity are related but distinct; one does not settle all three. |

## Questions for the live discussion

Confirmed scope from the live discussion is recorded in the [intended-product ticket](../issues/01-define-intended-product.md#comments). Keep decisions there rather than treating this source inventory as the product specification.

The remaining inventory needs Simon's decisions about which behaviors are required, optional, or excluded. In particular, existing evidence does not settle acquisition scope, the required live-session experience, or the extent of user/content portability. Ask one decision at a time. Existing product agreements are inputs, not questions to repeat without new evidence.

Image selection, default plugin selection, and detailed hardware support policy already have separate decision tickets. Do not resolve them by inference in this inventory.

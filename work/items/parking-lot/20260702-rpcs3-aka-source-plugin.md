---
id: 20260702-rpcs3-aka-source-plugin
title: Build RPCS3 source-machine plugin for Aka
type: feat
status: To Do
date: 2026-07-02
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/active/20260702-rpcs3-aka-source-plugin/work.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep the PS3 disc-folder discovery and Skate 3 case. The current published output map contains no RPCS3 package. The source record identifies the real PS3_DISC.SFB marker and records a historical firmware blocker.

## Scope on main

Recover a plugin for a device that can run RPCS3, not a permanent source-machine role. Preserve firmware refusal and the real disc-folder case. Recheck Aka and the publisher before implementation. Do not bundle or fetch firmware as part of this task.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

The recorded Aka firmware state is not fresh evidence. Discovery, launch, remote capture, and controller behavior each need acceptance.

## Supporting legacy evidence

The disc-folder discovery, firmware refusal, and Skate 3 requirements are in
`0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/active/20260702-rpcs3-aka-source-plugin/plan.md`.
That plan is historical. Its role names and data model do not override current contracts.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Build RPCS3 source-machine plugin for Aka

Plan and implement a Korri first-party RPCS3 plugin and Aka host wiring so PS3 titles in the Towada gaming library can be discovered, advertised by Aka, and launched through Korri's source-machine streaming path.

## Progress

- Added the first-party `@korri:rpcs3` plugin with PS3 disc-folder discovery for direct child `PS3_DISC.SFB` markers.
- Added RPCS3 readable launch materialization with absolute command, readable game/state roots, and firmware sentinel checks before spawn.
- Added opt-in NixOS source-machine wiring via `services.korri.rpcs3` and exposed it through `korri-source-machine`.
- Validated the real Towada library shape: `Skate 3 [BLUS30464]/PS3_DISC.SFB` is present as a direct marker.

## Live validation blocker

Aka does not currently have the default RPCS3 firmware sentinel at `/home/simonwjackson/.config/rpcs3/dev_flash/sys/external/liblv2.sprx` or `/var/lib/korri/rpcs3/dev_flash/sys/external/liblv2.sprx`. Live launch/stream validation remains blocked until firmware is installed manually into the state root; this work intentionally does not install or bundle PS3 firmware.

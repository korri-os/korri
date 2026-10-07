---
id: 01KVEQ0Z9G09F36SSMXA1H4T1P
slug: expose-full-steam-launch-lifecycle-observability
title: Expose full Steam launch lifecycle observability
origin: parked
status: To Do
priority: high
labels:
  - steam
  - observability
  - ui
  - session-lifecycle
created: 2026-06-19
source: user
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/active/01KVEQ0Z9G09F36SSMXA1H4T1P-expose-full-steam-launch-lifecycle-observability/item.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep the observed 30XX lifecycle signals as acceptance evidence for a future Steam integration. nix/product/published-plugins.nix contains no Steam output. Current launch status in services/korrid/src/host/session_state.rs describes generic unit state, not Steam download, shader, cloud, or prompt progress.

## Scope on main

Recheck the publisher before implementation. Preserve the observed event requirements, but do not restore the old sessiond or Effect-RPC endpoints. Any new wire contract needs its own decision.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

This depends on a usable Steam integration. Steam logs need privacy filtering, and progress cannot be inferred from process creation alone.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Expose full Steam launch lifecycle observability

## Why it matters

Steam launches emit rich state across download, shader/preflight, install scripts, cloud sync, prompts, process creation, Proton/FEX, game window, crash/exit, and cleanup, but today most of it is only visible by tailing Steam logs or process lists. Surfacing it as structured events lets the UI react accurately, explain waits/failures, and maximize user-visible progress signaling.

## Acceptance Criteria

- [ ] Steam plugin emits structured lifecycle events for app update/download, shader/pre-cache, install scripts, cloud sync, user prompts/interstitials, CreatingProcess, process added/updated/removed, Proton/FEX runtime setup, game window/running, crash, normal exit, and cleanup
- [ ] Events include appId, playable id when known, phase, status, progress when available, raw source/log line, timestamp, severity, and actionable hints
- [ ] korrid/sessiond expose a read-only lifecycle stream or query API consumable by Portal and tooling
- [ ] Portal shows current launch/download/shader/Steam status instead of only accepted/failed
- [ ] Tests cover parsing representative Steam console/log lines observed during 30XX launch

## Related

- `product/plugins/steam`
- `product/services/device/sessiond.ts`
- `product/apps/portal`
- `packages/pi-korrid-tools`

## Notes

Observed 30XX signals included CheckShaderDepotManifest, ProcessingInstallScript, SynchronizingCloud, ShowInterstitials, CreatingProcess, WaitingGameWindow, Completed, Game process added/updated/removed, first-run setup, Proton/FEX processes, and screenshot-verified main menu.

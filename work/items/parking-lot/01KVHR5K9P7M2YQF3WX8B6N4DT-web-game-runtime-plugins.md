---
id: 01KVHR5K9P7M2YQF3WX8B6N4DT
slug: web-game-runtime-plugins
title: "Web game runtime plugins (shared Chromium launcher + engine plugins)"
type: feat
status: To Do
created: 2026-06-19
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/active/01KVHR5K9P7M2YQF3WX8B6N4DT-web-game-runtime-plugins/work.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep one browser game runtime for the real Stargrove Scramble and Yoshi's Fabrication Station cases. The current published output map does not include these runtimes. The legacy plan explicitly targets Linux, not Android.

## Scope on main

Recover only the browser and engine behavior required by those games. Use the current declaration and launch boundaries in services/korrid/src/launcher/. The legacy --no-sandbox recipe is not approved. Keep browser sandboxing and plugin permissions in force.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

Browser startup, trusted input, canvas scaling, and performance still need physical acceptance. A portal kiosk is not a game browser runtime.

## Supporting legacy evidence

The detailed Linux cases and runtime requirements are in
`0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/active/01KVHR5K9P7M2YQF3WX8B6N4DT-web-game-runtime-plugins/plan.md`.
That plan is historical. Its config grammar and browser security flags are not current approval.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Web game runtime plugins

Origin: direct planning session (no upstream requirements doc). Converges the
ad-hoc Stargrove Chromium-under-gamescope work and the bespoke Yoshi's
Fabrication Station plugin onto one shared web runtime, with per-engine
normalization plugins (Option C: system-inferred launchers) on top.

Design sketches explored interactively this session live at `/tmp/web-runtime-sketch/`
(scratch, not committed).

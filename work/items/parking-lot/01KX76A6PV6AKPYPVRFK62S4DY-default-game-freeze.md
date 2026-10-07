---
id: 01KX76A6PV6AKPYPVRFK62S4DY
slug: freeze-host-game-by-default-on-moonlight-disconnect-and-clie
title: Freeze host game by default on Moonlight disconnect and client lid-close
origin: parked
status: To Do
priority: high
labels:
  - freeze-resume
  - streaming
  - sunshine
  - fakesuspend
created: 2026-07-10
source: user
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/active/01KX76A6PV6AKPYPVRFK62S4DY-default-game-freeze/item.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep automatic freeze on stream loss as separate policy work. Current exact-session freeze and thaw exist in services/korrid/src/host/session_state.rs and services/korrid/src/host/systemd_unit.rs. Remote input disconnection in services/korrid/src/host/input_coordination.rs removes a source; it does not implement this stream-loss policy.

## Scope on main

Reuse exact launch identity and the existing cgroup freezer. Do not restore fakesuspend, guessed active processes, or direct portal-to-peer calls. The production Linux viewer adapter remains a separate prerequisite in services/korrid/src/linux_viewer.rs.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

Network-loss detection latency, multiple connected players, and GPU/audio recovery after a long freeze require real acceptance. The historical one-hour criterion is not a current result.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Freeze host game by default on Moonlight disconnect and client lid-close

## Why it matters

When a Moonlight stream drops unintentionally (network) or the client lid closes, the host game today keeps burning CPU/GPU (or gets terminated by fakesuspend, losing state). Default behavior should be: freeze the host managed launch, thaw on reconnect. Both scenarios converge on one host-side watcher because a network-dropped client cannot signal; Sunshine on aka already logs CLIENT DISCONNECTED / New streaming session started, giving the detection signal. Mechanism already proven against Skate 3 (RPCS3+gamescope) on aka.

## Acceptance Criteria

- [ ] Host-side watcher (sessiond lifecycle hook or companion) observes Sunshine disconnect/reconnect signals and calls managed-launch freeze/thaw for the active launch.
- [ ] Hard network cut mid-stream results in the host game frozen (state T, ~0% CPU) within a bounded window; verify Sunshine's ungraceful-disconnect detection latency.
- [ ] Moonlight reconnect (New streaming session started) thaws the game before frames/input resume.
- [ ] bandai fakesuspend-controller sends best-effort remote freeze via the @korri:stream controlUrl instead of terminating the host game; falls back to host-side detection when network is already gone.
- [ ] Wake flow works end-to-end: lid open -> Moonlight relaunch -> session start -> host thaw -> gameplay resumes from frozen state.
- [ ] Long-freeze soak: game survives >=1h frozen and resumes cleanly (GPU fence/PipeWire recovery).
- [ ] Depends on: deploy of freeze endpoints to aka (push + mountainous flake bump) and RPC exposure item 01KX75XAWDVGPD7XW4V7MJ55EK.

## Related

- `product/services/device/fakesuspend-controller.ts`
- `product/services/device/overlay-remote-stop.ts`
- `product/services/device/sessiond.ts`
- `product/platform/plugin/session-lifecycle.ts`
- `work/items/parking-lot/01KX75XAWDVGPD7XW4V7MJ55EK-expose-managed-launch-freeze-thaw-through-effect-rpc-command.md`
- `work/items/parking-lot/01KX6M0HJK6AJCF7JC9XVKAZBH-upgrade-managed-launch-freeze-to-cgroup-v2-systemd-scopes.md`

---
id: 01M1S6KJXFBCQQ11SXT2BSBAQ6
slug: resolve-portal-wi-fi-packet-loss-before-declaring-sunshine-p
title: Resolve Portal Wi-Fi packet loss before declaring Sunshine production-ready
origin: parked
status: To Do
priority: high
labels:[]
created: 2026-09-05
source: se-debug
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktrees/feat/sunshine-v4l2m2m
  branch: feat/sunshine-v4l2m2m
---

# Resolve Portal Wi-Fi packet loss before declaring Sunshine production-ready

## Why it matters

The hardware encoder and standalone recovery IDRs pass at H.264/HEVC 720p and 1080p, but real Moonlight sessions lose initial video packets on the Portal WCN7850. Temporary TX checksum offload disable allowed all four static-console streams to render, but startup loss remained. The offload cause is not proven, and sustained moving-content throughput is unverified. This is a release blocker for a reliable Portal streaming device, not an encoder compilation failure.

## Acceptance Criteria

- [ ] Reproduce packet loss with client and server captures; distinguish checksum failure, burst loss, and capture/encoder faults.
- [ ] Run an on/off/on/off offload comparison and document whether checksum offload is causal; restore all diagnostic settings.
- [ ] Verify H.264 and HEVC 720p60/1080p60 moving content, recovery IDRs, and repeated reconnects through services/inputd/nix/korri-linux-host.nix.
- [ ] Land only the necessary device-specific correction with tested rollback; do not alter Android or boot partitions.

## Related

- `docs/acceptance/sunshine-korri-v4l2m2m-portal-2026-09-05.md`
- `nix/odin2portal/wifi.nix`
- `services/inputd/nix/korri-linux-host.nix`

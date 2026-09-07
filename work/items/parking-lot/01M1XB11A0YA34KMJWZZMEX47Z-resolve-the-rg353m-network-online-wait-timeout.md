---
id: 01M1XB11A0YA34KMJWZZMEX47Z
slug: resolve-the-rg353m-network-online-wait-timeout
title: Resolve the RG353M network-online wait timeout
origin: parked
status: To Do
priority: medium
labels:
  - rg353m
  - deployment
created: 2026-09-07
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktree/shared-portal-rpc
  branch: feat/shared-portal-rpc
---

# Resolve the RG353M network-online wait timeout

## Why it matters

Both the previous and new RG353M runtimes wait 120 seconds and fail systemd-networkd-wait-online during activation. The live portal and SSH can work while this wait fails. The timeout delays deployment and can delay services that require network-online. The portal work did not change networking.

## Acceptance Criteria

- [ ] Identify the actual interface and condition that keep network-online pending.
- [ ] Verify normal startup and activation without the 120-second timeout, including the usual disconnected USB case.
- [ ] Keep SSH, the portal, Sunshine, audio and input working. Do not mask the wait or broaden its exception without proving the required link policy.

## Related

- `docs/acceptance/shared-portal-2026-09-07.md`

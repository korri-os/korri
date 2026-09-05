---
id: 01M0M0WACPY5PVWK8A2HC8Z99J
slug: recover-zao-tsnet-proxy-authentication-after-maintenance-res
title: Recover Zao tsnet proxy authentication after maintenance restart
origin: parked
status: To Do
priority: medium
labels:
  - zao
  - tailscale
  - operations
created: 2026-08-22
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktrees/feat/restore-linux-inputplumber
  branch: feat/restore-linux-inputplumber
  commit: 8885dbf9
  repo: korri
  invoked_by: user
---

# Recover Zao tsnet proxy authentication after maintenance restart

## Why it matters

A required NixOS maintenance activation restarted the unrelated jellyfin and pyxis tsnet proxy services. Both now reject their stored node keys and retry. Korri does not depend on them, but the prior proxy availability is not restored.

## Acceptance Criteria

- [ ] Confirm whether the existing agenix auth key can safely reauthorize both proxies.
- [ ] Recover `tsnet-proxy-jellyfin.service` and `tsnet-proxy-pyxis.service` without exposing credentials.
- [ ] Verify both services remain active after a bounded restart and retain the intended tailnet hostnames.

## Related

- `mountainous/features`
- `work/items/active/019fde6b-8c02-7b01-8dfb-ffe97bcb5ef1-restore-linux-inputplumber/work.md`

## Notes

Do not couple this recovery to the Korri input rollout. The user explicitly asked to continue Korri without using tsnet proxies.

---
id: 01M1NGVGXPBTHFRDGZHNJQ1HYP
slug: route-remote-session-status-and-stop-through-secure-native-p
title: Route remote session status and stop through secure native peers
origin: parked
status: To Do
priority: medium
labels:
  - korrid
  - session
  - secure-peer
created: 2026-09-04
source: user
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktrees/feat/korrid-identity
  branch: feat/korrid-identity
  commit: 2a45bd531aafb85315efdeff20d714b55d22604c
  repo: korri
---

# Route remote session status and stop through secure native peers

## Why it matters

The Android brain can launch a secure native peer, but app.session.status and app.session.stop still use only the legacy session upstream. After Bandai started a Zao stream, both calls returned UpstreamFailure: no legacy session upstream configured, so the client could not cleanly stop the remote launch.

## Acceptance Criteria

- [ ] A secure native-peer launch can query app.session.status through the same selected peer.
- [ ] A secure native-peer launch can send app.session.stop with expected launch identity.
- [ ] Tests cover native-peer session status, normal stop, stale identity rejection, and force-stop authorization.
- [ ] The legacy session route remains compatible where configured.

## Related

- `services/korrid/src/lib.rs`
- `services/korrid/src/upstream_native.rs`
- `clients/android/app/src/main/java/com/limelight/KorriShellActivity.java`

---
id: 01M1WVVXS9S91BFP3WJ1XDGWGC
slug: land-the-portal-react-peer-resolution-test-fix-from-preview-
title: Land the portal React peer-resolution test fix from preview work
origin: parked
status: To Do
priority: medium
labels:
  - portal
  - testing
created: 2026-09-07
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri/.worktree/rg353m-gamepad-api
  branch: feat/rg353m-gamepad-api
---

# Land the portal React peer-resolution test fix from preview work

## Why it matters

The standard portal-check on base 1c4b13fe fails six unchanged OverlayRoot/SurfaceRoot cases with duplicate React invalid-hook errors. The preview worktree already contains a real-export module-resolution fix. Using that fix as a temporary preload makes all 257 portal tests pass; the controller slice deliberately does not take ownership of the preview session's uncommitted test change.

## Acceptance Criteria

- [ ] Land the preview owner's clients/portal/test/happydom.ts fix through its existing worktree.
- [ ] Run nix run .#portal-check without an external preload and verify the full suite passes.

## Related

- `clients/portal/test/happydom.ts`
- `clients/portal/src/overlay/OverlayRoot.test.tsx`
- `clients/portal/src/surface/SurfaceRoot.test.tsx`

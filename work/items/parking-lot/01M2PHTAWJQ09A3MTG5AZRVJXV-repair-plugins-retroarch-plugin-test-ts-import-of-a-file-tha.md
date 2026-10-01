---
id: 01M2PHTAWJQ09A3MTG5AZRVJXV
slug: repair-plugins-retroarch-plugin-test-ts-import-of-a-file-tha
title: Repair plugins/retroarch/plugin.test.ts import of a file that does not exist
origin: parked
status: To Do
priority: high
labels:
  - korrid-check
  - pre-existing
  - plugins
created: 2026-09-17
source: se-work
---

# Repair plugins/retroarch/plugin.test.ts import of a file that does not exist

## Why it matters

korrid-check stops at plugins/retroarch/check.sh, so every check after it never runs. That includes the portal check and the packaging gate. The gate reports a pass only because the failure is hidden behind a pipe in some invocations. Until this is fixed, korrid-check cannot prove the repository is green.

## Acceptance Criteria

- [ ] - `cd plugins/retroarch && bun run typecheck` exits 0 on main.
- `nix run .#korrid-check` reaches the portal check instead of stopping at the retroarch plugin.
- The `diagnostic` parameter on plugin.test.ts line 57 has an explicit type.

## Related

- `plugins/retroarch/plugin.test.ts`
- `plugins/libretro/retroarch.ts`

## Notes

plugin.test.ts line 6 reads `import { handlers } from "../mgba/retroarch"`. No `plugins/mgba/retroarch.ts` exists in any commit, including main at fbe4375c. `plugins/mgba/` holds only `.gitignore`, `README.md`, and `android/`. The likely intended target is `plugins/libretro/retroarch.ts`, which does exist and which `services/korrid/tests/plugin_registry.rs` already includes as RETROARCH_HELPER. Verified identical failure on main and on refactor/drop-android, so it is not caused by the Android removal.

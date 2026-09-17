---
id: 01M2Q3W8K5R7YT2ND4XJHB9VFB
slug: rewrite-scripting-md-for-a-korrid-that-ships-no-plugins
title: Rewrite SCRIPTING.md for a korrid that ships no plugins and no Android
origin: parked
status: To Do
priority: medium
labels:
  - docs
  - korrid
  - android
created: 2026-09-16
source: se-work
---

# Rewrite SCRIPTING.md for a korrid that ships no plugins and no Android

## Why it matters

`services/korrid/SCRIPTING.md` is the scripting contract readers and agents are
pointed at from `AGENTS.md`, and most of what it still describes is gone. It
documents bundled Android plugin sources, a built-in layer that enables
`@korri:android-app`, launcher/runtime/kind records, and two review tasks that
`1b3c9b27` retired. A contract document that describes a deleted platform teaches
the wrong model to everyone who reads it, including future sessions.

## Acceptance Criteria

- [ ] No section describes korrid as shipping or bundling plugin source
- [ ] No section describes Android packages, intents, PackageManager, or the
      `@korri:android-app` route as live behavior
- [ ] `nix run .#korrid-plugin-review` and `nix run .#korrid-plugin-route-review`
      no longer appear as available tasks
- [ ] Surviving Android material, if any is worth keeping, is marked as history
      with the `android-final` tag named

## Related

- `services/korrid/SCRIPTING.md` (lines 27, 182-200, 249-257, 367-369, 430-480)
- `docs/briefs/2026-09-16-android-removal-scope.md`
- Commits `241de7f8`, `1b3c9b27`

## Notes

Two sections were already removed during the Android cut; the rest of the document
was not revisited. The registry section also predates the plugin host owning
`/run/korri-plugin-host/enabled-packages.json` as the only registry korrid reads.

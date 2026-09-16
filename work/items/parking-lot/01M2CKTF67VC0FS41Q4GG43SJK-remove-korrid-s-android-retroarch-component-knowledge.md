---
id: 01M2CKTF67VC0FS41Q4GG43SJK
slug: remove-korrid-s-android-retroarch-component-knowledge
title: "Remove korrid's Android RetroArch component knowledge"
origin: parked
status: To Do
priority: medium
labels:
  - korrid
  - plugins
  - android
  - decoupling
created: 2026-09-13
source: user
context:
  branch: main
  repo: korri-os/korri
---

# Remove korrid's Android RetroArch component knowledge

## Why it matters

`services/korrid/src/launcher/retroarch.rs` carries the Android component route (package, activity, AndroidComponent) alongside the Linux one. Full decoupling means korrid special-cases neither: the Android RetroArch plugin must declare its own component the way the Linux plugin declares its argv and config. This half cannot be verified right now because no Android target is available, so it is deliberately separated from the Linux decoupling rather than landed blind.

## Acceptance Criteria

- [ ] No Android RetroArch package name, activity, or component constant appears in services/korrid/src/
- [ ] The Android RetroArch plugin declares its own component, and korrid launches it through the generic plugin route
- [ ] The existing Android app route gate passes on a real device before the change lands
- [ ] Deleting the Android RetroArch plugin leaves korrid building and passing its suite, with no Android launch capability

## Related

- `services/korrid/src/launcher/retroarch.rs`
- `plugins/retroarch/android/`
- `services/korrid/src/launcher/linux_plugin.rs`

## Notes

Owner confirmed 2026-09-12: RetroArch should be fully decoupled from korrid, Android included, but no Android device is available to test right now. Do the Linux slice first on the RG DS, then this.

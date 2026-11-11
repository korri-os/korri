---
status: accepted
---

# The Linux viewer stays a removable plugin, built from code in the Korri repository

**Owner decision, 2026-11-11.** Issue #391 replaces Moonlight Embedded with a
viewer that Korri owns. On Linux that viewer still ships as a removable
`@korri` plugin, as the owner decided on 2026-10-23. Its code lives in the
Korri repository, because the Android app in that repository uses the same
code.

## Considered options

- **Make the viewer part of Korri.** korrid and the viewer would always
  match. It reverses the 2026-10-23 decision, and the viewer's settings would
  need a new home outside the plugin settings cascade.
- **Keep the shared code in the plugins repository.** The Android app would
  then depend on the plugins repository, and the viewer's contract would sit
  apart from korrid's code.

## Consequences

- The plugins repository takes the Korri repository as a build input.
- korrid refuses a viewer plugin of the wrong version. It does not adapt to
  it.
- On Android the viewer is inside the app and is not a plugin.

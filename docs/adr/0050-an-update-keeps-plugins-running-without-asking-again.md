---
status: accepted
---

# An update keeps a device's plugins running without asking again

**Owner decisions, 2026-11-05.** Approval covers one exact package
(`services/korrid/plugin-host/README.md`), so every new release needed a new
approval, and a system update left installed plugins on old releases. On
2026-11-05 a system update of the RG353M made the plugin host refuse every
installed plugin. It saved them as off, rollback did not undo that, and SSH
would not have come back after a reboot.

## Decision

- A new release of a plugin the owner already approved is approved
  automatically when it asks for nothing more: no new native units, ports,
  devices, capabilities or hosts. A release that asks for more waits for the
  owner.
- After an update, the device checks itself: korrid is healthy, the shell shows
  a surface, and every plugin that was on is running. If any check fails, the
  device returns to the exact previous system, bundle and plugin state on its
  own, then tells the person.
- A plugin from another publisher that the new release cannot load stays
  installed and off, with a notice that says why. It comes back when its
  publisher ships a compatible release. The update never waits for outside
  authors.

## Consequences

- Each update snapshots plugin state, so that rollback can restore it.
  A rollback of the system generation alone does not.
- Plugin format changes such as ADR 0045's derived IDs are short-term API
  cleanups, not a recurring event. They get a one-off operational migration,
  not a lasting mechanism.

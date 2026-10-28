---
status: accepted
---

# Plugins declare only what Korri cannot derive

**Owner decisions, 2026-10-28.** The goal is a plugin that a person with no
Korri knowledge can write (ADR 0042).

- **Derived IDs.** A runner's ID is always `<pluginId>/<key>` and a system's ID
  is always its key. korrid refused any other value
  (`services/korrid/src/plugin.rs`), so authors typed these only to have them
  checked, and the runner ID put the publisher namespace inside plugin source.
  The `id` fields go in one clean cut. Every published plugin is rebuilt once,
  in the same rebuild as ADR 0043.
- **Default launch.** A runner with no `launch.prepare` handler runs its
  `program` with the content path as its only argument. It refuses authored
  settings and raw config overrides rather than dropping them, as
  `plugins/ppsspp` does. A runner that needs more writes its own handler, as
  before.

The cost is one launch convention owned by korrid. A runner that needs flags
and has no handler starts without them.

---
status: accepted
---

# A requirement without a version means the latest release

**Owner decisions, 2026-10-28.** Today a plugin requires another plugin by one
exact build (`services/korrid/plugin-host/README.md`, "Required plugin
graphs"). Each rebuild of a dependency then forces every plugin that requires
it to rebuild too. Across publishers that chain runs through different people.

## Decision

- A plugin may require plugins from any publisher, not only its own.
- A requirement names a plugin and can also name a release label or an exact
  build. A requirement that names only the plugin means its latest release,
  as a flake input with no revision does.
- The device resolves "latest" when the plugin is installed, and keeps that
  exact build until its owner updates. The owner approves the exact builds the
  device resolved, so approval still covers exact files.
- Each plugin repository marks one release per plugin as its latest. The
  publish command sets the mark. Release labels keep no implied order.
- If two installed plugins need different releases of one plugin, the device
  refuses the second install and names both plugins and both releases. A
  device still holds one release of each plugin.
- Chains can go to any depth within today's limits: 128 plugins, 1024 links
  and no cycles. One approval screen lists every publisher an install brings.

## Consequences

Two devices can hold different builds of the same plugin. A breaking change in
a dependency can break the plugins that require it when the owner updates;
#325 covers the contract stability promise. An install can ask the owner to
trust several publishers at once.

The spelling of a requirement, the latest mark in the catalog, and whether a
root update also updates its dependencies are not decided.

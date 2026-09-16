---
id: 01M2M26ZM9HKYM437SFJ63NRCX
slug: make-korrid-check-s-brain-smoke-run-without-an-installed-plu
title: "Make korrid-check's brain smoke run without an installed plugin host"
origin: parked
status: To Do
priority: high
labels:
  - korrid
  - checks
  - pre-existing
created: 2026-09-16
source: se-work
---

# Make korrid-check's brain smoke run without an installed plugin host

## Why it matters

nix run .#korrid-check fails identically on main (0a30ad4e) and on the plugin-model branch with "korrid smoke did not exercise brain-only local games". Standalone korrid resolves plugins through RegistrySource::Installed, which reads /run/korri-plugin-host/enabled-packages.json. That file only exists where the plugin-host service has run, so the repository's own check cannot pass on an ordinary development machine. This blocks the main pre-merge gate for every plugin change and hides real regressions behind a known-red check.

## Acceptance Criteria

- [ ] korrid-check passes on a development machine with no plugin host installed
- [ ] The brain smoke still proves local-games listing against the checkpoint fixture
- [ ] A missing registry is either provisioned by the check or handled as an explicit no-installed-plugins state, decided deliberately rather than by silent fallback

## Related

- `services/korrid/check-in-shell.sh`
- `services/korrid/src/plugin_installation.rs`
- `services/korrid/src/plugin_policy.rs`

## Notes

Verified by running the check on both trees: same failure message, same point. The plugin-model branch does not change registry_source or the registry reader.

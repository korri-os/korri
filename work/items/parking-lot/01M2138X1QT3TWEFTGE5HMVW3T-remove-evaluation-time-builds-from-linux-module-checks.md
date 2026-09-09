---
id: 01M2138X1QT3TWEFTGE5HMVW3T
slug: remove-evaluation-time-builds-from-linux-module-checks
title: Remove evaluation-time builds from Linux module checks
origin: parked
status: To Do
priority: medium
labels:
  - nix
  - ci
created: 2026-09-08
source: se-work
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  branch: ci/garnix-cache
  commit: ca06b94a
  repo: simonwjackson/korri
---

# Remove evaluation-time builds from Linux module checks

## Why it matters

The korrid-linux-device-module and korri-linux-host-module checks read generated helper derivations during Nix evaluation. They cannot join the initial Garnix evaluation-only check list. Move those reads into check build phases without reducing behavioral assertions so cross-architecture CI can include the checks without evaluator builds.

## Acceptance Criteria

- [ ] Both check derivation paths evaluate for x86_64-linux and aarch64-linux with allow-import-from-derivation=false, max-jobs=0, and no builders.
- [ ] The checks retain their existing owner-binding and software-readiness assertions and pass on the relevant build architectures.
- [ ] Add the verified checks to garnix.yaml after the evaluation-time builds are removed.

## Related

- `services/korrid/nixos-module-check.nix`
- `services/inputd/nix/korri-linux-host-module-check.nix`
- `garnix.yaml`

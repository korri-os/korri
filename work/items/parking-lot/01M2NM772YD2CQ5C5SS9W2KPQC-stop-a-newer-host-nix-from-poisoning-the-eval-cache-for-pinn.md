---
id: 01M2NM772YD2CQ5C5SS9W2KPQC
slug: stop-a-newer-host-nix-from-poisoning-the-eval-cache-for-pinn
title: Stop a newer host Nix from poisoning the eval cache for pinned-Nix tasks
origin: parked
status: To Do
priority: medium
labels:
  - nix
  - checks
  - developer-experience
  - pre-existing
created: 2026-09-16
source: se-work
---

# Stop a newer host Nix from poisoning the eval cache for pinned-Nix tasks

## Why it matters

Korri's Nix apps pin Nix 2.31.2 through runtimeInputs, but an interactive shell may run a newer Nix (2.34.1 here). Both share ~/.cache/nix/eval-cache-v6. Once the newer Nix writes entries for a flake revision, the pinned 2.31.2 fails to read them and reports "experimental Nix feature 'dynamic-derivations' is disabled", which names a feature nothing in the repo uses. nix run .#nixos-layout-check then fails on rg353m for a reason that has nothing to do with the code under test. I lost a verification cycle to it and briefly blamed my own commit: with the cache wiped, both main's tip and the branch pass. Any contributor whose host Nix is newer than the pin will hit the same false failure.

## Acceptance Criteria

- [ ] nix run .#nixos-layout-check gives the same result regardless of the host Nix version in PATH
- [ ] Either the tasks isolate their eval cache, or they pass --no-eval-cache, or the pin matches the devshell Nix
- [ ] The failure mode is documented so the misleading dynamic-derivations message is recognisable

## Related

- `nix/tasks.nix`
- `nix/base/wifi-check.sh`

## Notes

Reproduced: app nix 2.31.2 fails reading a cache written by 2.34.1, and passes with --no-eval-cache at the same commit. After `mv ~/.cache/nix/eval-cache-v6` aside, nixos-layout-check passed at both 567b8014 (main) and 951cd42a (branch). Host nix 2.34.1; pinned app nix /nix/store/i5k130pbirxlpva6mqz06v4nh7lb452y-nix-2.31.2+1.

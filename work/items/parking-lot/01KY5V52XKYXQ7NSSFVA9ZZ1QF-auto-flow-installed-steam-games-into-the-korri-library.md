---
id: 01KY5V52XKYXQ7NSSFVA9ZZ1QF
slug: auto-flow-installed-steam-games-into-the-korri-library
title: Auto-flow installed Steam games into the Korri library
origin: parked
status: To Do
priority: medium
labels:
  - steam
  - library
  - discovery
  - scanner
created: 2026-07-22
source: user
context:
  cwd: /home/simonwjackson/code/sandbox/korri
  repo: korri
---

# Main applicability

Imported from `0e4cec9da3d77e6578b8a01a5d83420ba0d98e62:work/items/parking-lot/01KY5V52XKYXQ7NSSFVA9ZZ1QF-auto-flow-installed-steam-games-into-the-korri-library.md`.
Source inspection used main `04d300680184e4db6a58e86514284c9b342159c4`. No new runtime test or device acceptance was performed.

## Why keep this

Keep the installed-manifest acceptance requirement for a future Steam plugin. The current published output map contains no Steam package, and FolderScanner uses enabled plugin file claims rather than a Steam installed-manifest producer.

## Scope on main

Recover a bounded installed-app producer from legacy when Steam support is selected. Recheck the publisher first. Do not restore the runaway whole-filesystem scan or copy the old authored catalog schema.

This is parked work, not an approved implementation plan. `AGENTS.md`, current contracts, and current producer data govern future work. Historical schemas, paths, APIs, safety settings, and completed boxes below do not establish current main behavior.

## Cost and remaining acceptance

An installed manifest does not prove that a game launches. Install, uninstall, and partially downloaded states need separate tests.

## Legacy record

The original body follows unchanged. Its progress and acceptance refer to legacy. Retrieve related files from the same fixed commit, not from current main.


# Auto-flow installed Steam games into the Korri library

## Why it matters

Newly-installed Steam games do not appear in the Korri library. The library is hand-curated in korri.yaml (Steam entries are authored with their AppID), and the automatic release scan is disabled for deploy safety plus a known runaway bug (01KWN0HSZV). A fully-installed, launchable game (Roundguard 848030, StateFlags 4) was invisible in the GUI until manually authored. Users reasonably expect installed Steam titles to show up without hand-editing config.

## Acceptance Criteria

- [ ] Installing a Steam game (appmanifest present, StateFlags fully-installed) surfaces it in the Korri library without manual korri.yaml edits.
- [ ] The Steam discovery provider's installed-manifest results merge into the catalog on a safe, bounded trigger (not the runaway full-filesystem scan).
- [ ] Removing/uninstalling a Steam game removes or marks it in the library.

## Related

- `product/plugins/steam/src/discovery.ts`
- `product/platform/library/discovery/release-candidate-scan.ts`
- `var/lib/korri/config/korri.yaml`
- `01KWN0HSZV6CFQ7MT8MDMXR52S`

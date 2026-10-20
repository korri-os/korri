---
status: proposed
---

# The device korrid runs discovery and the store

**Owner decision, 2026-10-20 (option a).** The korrid on a device runs
discovery and the store on that device's own installed library. One korrid
finds, downloads and launches a game. This amends the discovery v2 contract
(`docs/briefs/2026-10-16-discovery-v2-contract.md`), which said "Brain mode
only".

## Problem

Every device runs `KORRID_MODE = "host"`
(`services/korrid/nixos-module.nix`) with an installed library root
(`KORRID_STORAGE_ROOT`). Discovery, its library writer and the store
(ADR 0014) ran only in brain mode, and a brain cannot launch installed games.
So on a device:

- `app.discovery.*` and the attention RPCs answered `OperationUnsupported`;
- the six `app.acquisition.*` store RPCs answered `AcquisitionUnavailable`;
- an ADR 0013 declared-artifact game never reached the library, because only
  a discovery pass writes it.

Yet devices already depend on discovery's output: `korrid catalog migrate`
writes paired sources under `games/`, and
`nix/product/game-library-access.nix` (99f149991) gives the game user access
to those folders.

## Decision

- A host with an installed library root builds one
  `DiscoveryLifecycleCoordinator` when its RPC state is built. It uses the
  same library root, private state root, publication lock
  (`HostRuntime.route_write_lock`, the lock settings and runner choices
  already take) and registry source as the launch routes.
- One code path: the discovery RPCs (snapshot, rescan, pair, change, unpair),
  the attention RPCs (list, assign, ignore) and the six store RPCs resolve the
  device's library through one accessor, `AppState::local_library`, in both
  modes. The "only from the local brain" refusals are gone; no host branch is
  added.
- Owner checks do not change. Pair, change and unpair need the caller to act
  for the device's verified owner, with the same rule brain mode used
  (`brain_caller_is_owner`, now `caller_is_owner` against the device's owner
  key).
- A host with no installed library root still answers `OperationUnsupported`
  (discovery, attention) and `AcquisitionUnavailable` (store).
- Discovery writes `device.yaml` and `catalog/library.yaml` in the library
  root. The host's launch routes read those same files on every request, so a
  game that discovery or the store adds is launchable by the same process
  with no restart.

This ADR does not rename host mode and does not extend it as a concept
(AGENTS.md). It moves one capability into the shared path, which is a step
toward retiring the host/brain split.

## Considered Options

- **(b) The brain launches installed games.** This would move the launcher,
  the session journal, the input seat and the systemd units into brain mode.
  It is a larger cut, and it deepens the split instead of shrinking it.
- **(c) Two korrids on each device,** a brain for discovery and the store and a
  host for launches. They would share one library root, which means two
  writers, two publication locks and cross-process coordination, and the
  portal would talk to two brains. That contradicts "the portal's brain is
  always the korrid on its own device".
- **(d) Defer.** Devices could then never show store or ADR 0013 games, and
  the portal's download work (30a241cac..0dee19a6e) would have no device to
  run on.

## Consequences

- **Startup cost.** Building the coordinator reads `device.yaml` and
  evaluates the installed plugin registry once, synchronously: the same
  evaluation every route request already performs. The first discovery scan
  then runs on its own thread. It walks every paired source and hashes only
  new or changed files; unchanged files cost a stat each, through the hash
  cache in the private state root. The first scan on a device that has never
  been scanned hashes every file once. korrid answers the portal while that
  scan runs: the snapshot reports `Scanning`, and launches use the documents
  as they are.
- **Concurrent writers.** Discovery, settings and runner choice share the
  host's publication lock. Writes from outside korrid are still detected by
  revision, as in brain mode.
- **Folder receipts.** A device korrid has no folder picker that issues
  folder-selection receipts (the brain's embedding API
  `issue_folder_selection_receipt` is the only issuer). Until one exists,
  `app.discovery.pair` on a device can only consume receipts that something
  else issued. Sources written by `korrid catalog migrate`, rescan, change,
  unpair and attention all work.
- ADR 0013 declared-artifact games appear in a device's library after the
  first scan. ADR 0014 store downloads are recorded on the device and launch
  from its cache.

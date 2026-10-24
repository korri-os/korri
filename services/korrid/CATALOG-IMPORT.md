# Library folders

Use the live person CLI, `korri`, for ordinary library changes. Keep korrid
running. Run as the device's person account or root, on that device:

```sh
korri library folders list
korri library folders add /absolute/path/to/gba-files gba
korri library folders rescan
korri library attention list
korri library folders remove gba --yes
```

Install and enable approved plugins first. Their declarations supply systems
and system groups. A library folder pairs with exactly one of these; the
folder name never chooses its system. Discovery can identify content even
when no enabled runner can play it.

Add obtains a short-lived, one-use folder-selection receipt through the
protected local person socket. It does not use a path as a token. Remote
selection is refused; use SSH or local access on the device. Overlapping
folders and plugin-data folders are refused. List returns the actual folder
ID. It starts with the system or group ID and gains a numeric suffix on a
clash. Use that exact ID for removal.

Remove unregisters the library folder and removes discovery-owned records,
not game files or play logs. Edited and hand-authored records stay. Without
`--yes`, removal asks for confirmation only on a terminal. Piped commands
need explicit confirmation. `--json` prints the native RPC response.

Only registered library folders are scanned. Declared games are checked at
their plugin-declared account-relative paths; discovery never walks `users/`.
Add and rescan publish the existing `device.yaml`, `catalog/library.yaml` and
private discovery state through the conflict-checked coordinator. Unchanged
rescans preserve document bytes. The wire vocabulary cut does not rename
persisted `sources`, storage IDs, paths or attention decisions.

Scan responses can be Scanning or Enriching before they settle. Success does
not mean every file was identified or a launch route is available. Inspect
`korri library attention list` and resolve with `assign` or `ignore`.
Configuration parse details are withheld because files can contain credentials.

## Hidden one-off migration from games.yaml and releases.yaml

`korrid catalog migrate` remains operational tooling until every device's
cutover is confirmed. It is not a live person command. The old `korrid catalog
pair`, `unpair` and `rescan` commands are removed, with no aliases.

Stop the device's korrid and all other catalog writers before migration. Keep
them stopped until the command exits. Run only one command at a time: the
coordinator's in-process lock does not exclude another process. Run as the
account that owns the daemon's readable and private roots. Back up both roots.
Both roots must exist; no defaults are accepted.

```sh
: "${KORRID_STORAGE_ROOT:?Set the daemon's existing storage root}"
: "${KORRID_PRIVATE_STATE_ROOT:?Set the daemon's existing private state root}"
export KORRID_STORAGE_ROOT KORRID_PRIVATE_STATE_ROOT
korrid catalog migrate --plan /private/path/discovery-plan.json
# Review kept, dropped, unresolved, moves, sources and play-log renames.
korrid catalog migrate --apply --plan /private/path/discovery-plan.json
```

Dry run writes a private plan without moving files or changing the catalog.
Apply moves files, pairs the planned folders, removes superseded catalog and
private discovery records, scans once, carries titles through the catalog
publisher, and renames play logs. Unresolved files remain. Any error stops
before the next step. The report beside the plan lists completed moves for
recovery. Review completed moves and backups before any retry. There is no
runtime fallback to retired documents or commands.

Restart korrid after reviewing the report. On a publication failure, keep the
same roots so the coordinator can recover its journal.

## Linux gameplay access

Neither live folder changes nor the migration grant gameplay access. Validate
traversal, game reads and writable saves separately. Keep readable
configuration and private state inaccessible to games. Existing files can need
explicit ACL changes; discovery does not change ACLs.

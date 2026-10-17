# Offline catalog discovery

Stop the device's korrid daemon before a catalog change. Keep it stopped until
all commands exit. Run only one command at a time. The coordinator's lock is
in-process; it does not stop another process.

Install and enable the approved plugins first. Discovery uses their content
rules, even when no enabled runner can play the identified system.

Run as the account that owns the daemon's readable and private roots. Both
roots must exist. No defaults are accepted.

```sh
: "${KORRID_STORAGE_ROOT:?Set the daemon's existing storage root}"
: "${KORRID_PRIVATE_STATE_ROOT:?Set the daemon's existing private state root}"
export KORRID_STORAGE_ROOT KORRID_PRIVATE_STATE_ROOT
korrid catalog pair /absolute/path/to/gba-files gba
korrid catalog rescan
korrid catalog unpair gba
```

`pair` registers a folder with one plugin-defined system or group. The returned
source id starts with that system or group id and gains a numeric suffix on a
clash. Use that id with `unpair`. Overlapping folders and plugin-data folders
are refused. The folder's name never chooses its system.

Only paired sources are scanned. Declared prepared games are checked at their
plugin-declared account-relative paths; discovery never walks `users/`.
Pairing and rescanning publish `device.yaml`, `catalog/library.yaml`, and private
discovery state through one conflict-checked coordinator. Unchanged rescans
preserve document bytes. Unpair removes discovery-owned records, not game files
or play logs. Back up both roots before operator changes.

The report gives unit, hashed-byte, added-game, removed-location, repair, and
diagnostic counts. A successful exit does not mean every file was identified or
that a launch route is available. Inspect attention items through the local RPC.
Configuration parse details are withheld because files can contain credentials.

## One-off migration from games.yaml and releases.yaml

Back up both roots before apply. Dry run writes a private plan and does not move
files or change the catalog:

```sh
korrid catalog migrate --plan /private/path/discovery-plan.json
# Review kept, dropped, unresolved, moves, sources, and play-log renames.
korrid catalog migrate --apply --plan /private/path/discovery-plan.json
```

Apply moves files, pairs the planned sources, removes superseded catalog and
private discovery records, scans once, carries titles through the catalog
publisher, and renames play logs. Unresolved files remain in place. Any error
stops before the next step. The report beside the plan lists completed moves
for recovery. There is no runtime fallback to the retired documents or commands.

## Linux gameplay access

These commands grant no gameplay access. Validate traversal, game reads, and
writable saves separately. Keep readable configuration and private state
inaccessible to games. Existing files can need explicit ACL changes; discovery
does not make those changes.

The command opens no HTTP listener and grants no browser folder-selection
authority. Restart the daemon after reviewing the report. On a publication
failure, keep the same roots so the coordinator can recover its journal. Do not
retry a failed migration without reviewing its completed moves and backups.

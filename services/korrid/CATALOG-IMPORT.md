# Offline catalog import

Stop the device's korrid daemon before import. Keep it stopped until the command
exits. Do not run two imports at once. The coordinator's locks are in-process;
this command does not detect or stop another process.

Copy the GBA files into their intended persistent directory first. Run the command
as the same account that owns the daemon's catalog and private state. Set both
roots to the existing directories used by that daemon; no defaults are accepted.

```sh
: "${KORRID_STORAGE_ROOT:?Set the daemon's existing storage root}"
: "${KORRID_PRIVATE_STATE_ROOT:?Set the daemon's existing private state root}"
export KORRID_STORAGE_ROOT KORRID_PRIVATE_STATE_ROOT
korrid catalog import /absolute/path/to/copied-gba-directory
```

The command calls production `DiscoveryCoordinator.add_location`. That producer
owns the storage record, content hashes, library releases, and private discovery
state. Do not author catalog YAML for this operation. The selected directory must
exist. The command registers files in place; it does not copy, move, or delete
them. Keep that directory available after import.

Import rescans **all configured locations**, not only the selected directory.
Repeated import uses the coordinator's existing identity and hash-cache rules.
It can reconcile discovery-owned records for other locations. Back up the
catalog and private state before an operator import.

The report gives candidate, hashed-byte, added-record, removed-record,
removed-release, repair, and diagnostic counts from the coordinator. Diagnostics
can mean that files were skipped; a successful exit does not mean every file was
imported or that a launch route is available. Only enabled plugin claims are
used. The command is not limited to GBA files.

## Linux gameplay access

Import runs as the catalog owner; it does not automatically make ROMs readable
by the gameplay identity. Validate traversal, ROM reads and writable save paths
separately. Keep configuration and private state inaccessible to games.

The RG353M composition explicitly imports `nix/rg353m/gba-gameplay.nix`. Its
access module grants named traversal/read ACLs and account save access, denies
gameplay reads of existing root YAML documents, and adds a default deny for
new private descendants. Default ACLs do not retroactively protect existing
trees; inspect those before deployment. Existing copied ROM files require
explicit read ACLs; the default applies only to subsequent files.

The generated Linux RetroArch configuration uses **Select+Start to exit** and
save automatically. Guide remains reserved for Korri. The board profile keeps
schedutil and limits the CPU ceiling to the device-tested 1.104 GHz.

The command opens no HTTP listener, starts no daemon, and grants no browser
folder-selection authority. Restart the daemon after reviewing the report. On
failure, keep the same roots when retrying so the coordinator can use its existing
repair state. Configuration parse details are withheld from output because they
can contain credentials; inspect those files privately.

# Title-cleaning migration (one-off, ADR 0090)

Since ADR 0090, discovery cleans every captured title before it stores it:
a system plugin's `title.clean` handler answers first, and Korri's generic
cleaning (`services/korrid/src/discovery/title.rs`) applies when no plugin
answers. Games that discovery added before the cut still hold the raw file
name as their title, for example `Secret of Mana (U) [!]`. This script cleans
those titles once, on the device, when the cut is deployed. korrid has no
runtime migration for them (AGENTS.md: one clean cut).

## What it changes

- It reads `device.yaml` and `catalog/library.yaml` under the storage root.
  It writes only `catalog/library.yaml`, and only the `title` of a game.
- It changes a game only when one of its releases has a discovered location
  (`File` or `FileSet` with `discovery`), and the stored title is the one
  discovery wrote from that file name: the captured `{title}` (a part of
  the name), or the old `fallback_title` of the name.
- For a `FileSet`, the name is the descriptor (`.cue`, `.m3u`) when the set
  has one, otherwise the first file of the set.
- A title that came from a person, a plugin declaration or acquisition
  stays. The dry run lists each one as `kept`.
- It applies Korri's generic cleaning, the same rules as `title.rs`. Both
  test against `services/korrid/test-data/title-cleaning.json`. It does not
  run plugins: no system plugin had a `title.clean` handler at the cut.
- The raw name stays in the release's location in `device.yaml`.

## Run it

Do not run it before the korrid build with ADR 0090 is deployed.

```sh
sudo systemctl stop korrid
sudo ./migrate.py --storage-root /var/lib/korri --dry-run
sudo ./migrate.py --storage-root /var/lib/korri
sudo systemctl start korrid
```

- `--dry-run` prints each change (`id: 'old' -> 'new' (from 'file')`) and
  each kept title. It writes nothing.
- A real run first copies `catalog/library.yaml` to
  `title-cleaning-backup-<UTC time>/catalog/library.yaml` under the storage
  root, and checks that the copy matches the original byte for byte. It
  stops before any write if the copy differs. It then replaces the file
  atomically with the same owner, mode and extended attributes.
- A second run finds nothing to change and makes no backup.
- It refuses to run while a `korrid` process runs.

To undo it, stop korrid and copy the backup back to
`catalog/library.yaml`.

## Art after the migration

A cleaned title changes the game's record, and so its release fingerprint.
The fingerprint keys art attempts and art assignments (ADR 0032, #464
amendment: a title or release-list edit invalidates the assignment). So
every cleaned game hides its stored art until the next art pass fetches art
for it again. The old images stay on disk; they are not shown.

The art pass runs after each successful discovery pass. To start one:

```sh
korri library folders rescan
```

Each pass asks one art plugin about at most 8 games. Run the rescan again
until every cleaned game has its art back; `korri library games list` shows
the library.

## Tests

```sh
./test_migrate.py
```

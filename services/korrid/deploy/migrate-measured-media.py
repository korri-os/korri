#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
"""One-off ADR 0091 migration of a device's stored art. Not korrid code.

korrid no longer stores one image per fixed role (`tile`, `poster`, `hero`,
`logo`). It keeps up to four images per plugin, game and kind (`logo`,
`picture`, `icon`), and it asks every art plugin again on its own schedule.
Run this once on each device when that korrid is deployed, with korrid
stopped, as root or as the user that owns korrid's private state root
(`/var/lib/korrid` on devices):

    systemctl stop korrid
    ./migrate-measured-media.py --private /var/lib/korrid          # dry run
    ./migrate-measured-media.py --private /var/lib/korrid --apply
    systemctl start korrid

What it changes, all under the private root:

1. `game-assets/assignments.json`: each record
   `<playable id>:<role>` = `{asset_id, role, owner, source, color?}`
   becomes `<playable id>:<plugin id>:<kind>:<n>` =
   `{asset_id, kind, owner, source, color?}`, where `<plugin id>` is the
   record's `source.provider`. `tile`, `poster` and `hero` become `picture`
   and `logo` stays `logo`. `<n>` counts from 0 per playable id, plugin and
   kind, in the order tile, poster, hero. `owner`, `source` and `color` do
   not change. The images are not checked again against korrid's new rules.
2. `<plugin id>/attempts.json`, for each plugin folder that holds one: renamed
   to `attempts.json.before-measured-media`. Its records have no answer time
   and no public-ids hash, so korrid cannot read them. Without them, korrid
   asks each art plugin about each game once more, at most 8 games per plugin
   per pass, and keeps what it gets beside the images it already has.

`game-assets/blobs/` does not change, so no asset id changes. A device's
`coverAssetId` stays the same: the old tile is square, so it is the largest
square-ish picture of its game unless the game has no other one.

It is a dry run unless `--apply` is given. It checks everything before it
writes anything, keeps one backup of the old `assignments.json` beside it
(`assignments.json.before-measured-media`), and refuses to run on a file that
is already migrated or that holds a record it does not know. A run that
stopped part way is finished by the next run. It prints what it changed.

Check afterwards: korrid starts, the local games list reports the same
`coverAssetId` values as before, and each game's `media` lists the same
images with `kind` in place of `role`.
"""
import argparse
import json
import os
from pathlib import Path
import stat
import sys
import tempfile

ASSIGNMENTS = Path('game-assets') / 'assignments.json'
ATTEMPTS = 'attempts.json'
BACKUP_SUFFIX = '.before-measured-media'
KIND_OF_ROLE = {'tile': 'picture', 'poster': 'picture', 'hero': 'picture', 'logo': 'logo'}
ROLE_ORDER = ('tile', 'poster', 'hero', 'logo')
KINDS = ('logo', 'picture', 'icon')
OLD_FIELDS = {'asset_id', 'role', 'owner', 'source'}
NEW_FIELDS = {'asset_id', 'kind', 'owner', 'source'}


class Refused(Exception):
    """The state is not one this migration may change. Nothing was written."""


def fields_are(record, required):
    return set(record) in (required, required | {'color'})


def playable_id(record):
    owner = record.get('owner')
    return owner.get('playable_id') if isinstance(owner, dict) else None


def provider(record):
    source = record.get('source')
    return source.get('provider') if isinstance(source, dict) else None


def is_old(key, record):
    return (fields_are(record, OLD_FIELDS) and record['role'] in KIND_OF_ROLE
            and isinstance(playable_id(record), str) and isinstance(provider(record), str)
            and key == f"{playable_id(record)}:{record['role']}")


def is_new(key, record):
    if not (fields_are(record, NEW_FIELDS) and record['kind'] in KINDS
            and isinstance(playable_id(record), str) and isinstance(provider(record), str)):
        return False
    prefix = f"{playable_id(record)}:{provider(record)}:{record['kind']}:"
    return key.startswith(prefix) and key[len(prefix):].isdigit()


def migrate_assignments(data):
    """The new `assignments.json` bytes and the number of records rewritten.

    `(data, 0)` when every record is already in the new form. A file that
    mixes both forms, or holds a record of neither form, is refused whole.
    """
    try:
        records = json.loads(data)
    except ValueError:
        raise Refused('assignments.json is not valid JSON') from None
    if not isinstance(records, dict) or not all(isinstance(r, dict) for r in records.values()):
        raise Refused('assignments.json is not an object of records')
    old = [key for key, record in records.items() if is_old(key, record)]
    new = [key for key, record in records.items() if is_new(key, record)]
    if len(old) + len(new) != len(records):
        raise Refused('assignments.json holds a record this migration does not know')
    if not old:
        return data, 0
    if new:
        raise Refused('assignments.json mixes records from before and after the migration')
    ordered = sorted(records.values(),
                     key=lambda record: (playable_id(record), ROLE_ORDER.index(record['role'])))
    migrated = {}
    places = {}
    for record in ordered:
        kind = KIND_OF_ROLE[record['role']]
        group = (playable_id(record), provider(record), kind)
        place = places.get(group, 0)
        places[group] = place + 1
        rewritten = {name: value for name, value in record.items() if name != 'role'}
        rewritten['kind'] = kind
        migrated[f'{group[0]}:{group[1]}:{kind}:{place}'] = rewritten
    return (json.dumps(migrated, indent=2, sort_keys=True) + '\n').encode(), len(old)


def regular_file(path):
    return path.exists() and not path.is_symlink() and path.is_file()


def write_like(path, data, metadata):
    """Replace `path` with `data`, keeping the mode and owner in `metadata`."""
    fd, name = tempfile.mkstemp(prefix='.korri-media-', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as output:
            os.fchmod(output.fileno(), stat.S_IMODE(metadata.st_mode))
            if os.geteuid() == 0:
                os.fchown(output.fileno(), metadata.st_uid, metadata.st_gid)
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)
    sync_directory(path.parent)


def sync_directory(path):
    directory = os.open(path, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def attempt_files(private):
    """Each plugin folder's `attempts.json`. Plugin ids start with `@`."""
    found = []
    for folder in sorted(private.iterdir()):
        if not folder.name.startswith('@') or folder.is_symlink() or not folder.is_dir():
            continue
        attempts = folder / ATTEMPTS
        if attempts.exists() or attempts.is_symlink():
            if not regular_file(attempts):
                raise Refused(f'{folder.name}/{ATTEMPTS} is not a regular file')
            backup = attempts.with_name(ATTEMPTS + BACKUP_SUFFIX)
            if backup.exists() or backup.is_symlink():
                raise Refused(f'{folder.name}/{backup.name} already exists')
            found.append(attempts)
    return found


def run(private, apply=False):
    """Check everything, then change it when `apply`. Returns the report."""
    if private.is_symlink() or not private.is_dir():
        raise Refused('the private state root is not a directory')
    owner = private.stat()
    if os.geteuid() not in (0, owner.st_uid):
        raise Refused('run as root or as the owner of the private state root')

    assignments = private / ASSIGNMENTS
    backup = assignments.with_name(assignments.name + BACKUP_SUFFIX)

    # Everything is checked before anything is written.
    original = updated = None
    rewritten = 0
    if assignments.exists() or assignments.is_symlink():
        if not regular_file(assignments):
            raise Refused('game-assets/assignments.json is not a regular file')
        original = assignments.read_bytes()
        updated, rewritten = migrate_assignments(original)
        # A backup left by a run that stopped before the rewrite is this same
        # old file. Any other file of that name is not ours to replace.
        if rewritten and (backup.exists() or backup.is_symlink()) and not (
                regular_file(backup) and backup.read_bytes() == original):
            raise Refused(f'{backup.name} already exists with other content')
    attempts = attempt_files(private)

    if not (rewritten or attempts):
        migrated = bool(original and json.loads(original))
        if migrated:
            raise Refused('this private state root is already migrated')
        return {'applied': False, 'changes': [], 'note': 'nothing to migrate'}

    changes = []
    if rewritten:
        changes.append(f'{ASSIGNMENTS}: {rewritten} record(s) rewritten as '
                       f'<playable id>:<plugin id>:<kind>:<n>; backup {backup.name}')
    for path in attempts:
        changes.append(f'{path.parent.name}/{ATTEMPTS}: renamed to {ATTEMPTS}{BACKUP_SUFFIX}; '
                       'korrid asks that plugin about each game again')
    if not apply:
        return {'applied': False, 'changes': changes}

    if rewritten:
        if assignments.read_bytes() != original:
            raise Refused('assignments.json changed during the migration; is korrid stopped?')
        metadata = assignments.stat()
        if not backup.exists():
            fd = os.open(backup, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, 'wb') as output:
                os.fchmod(output.fileno(), stat.S_IMODE(metadata.st_mode))
                if os.geteuid() == 0:
                    os.fchown(output.fileno(), metadata.st_uid, metadata.st_gid)
                output.write(original)
                output.flush()
                os.fsync(output.fileno())
        if backup.read_bytes() != original:
            raise Refused(f'{backup.name} does not hold the old file; nothing was rewritten')
        write_like(assignments, updated, metadata)
    for path in attempts:
        os.rename(path, path.with_name(ATTEMPTS + BACKUP_SUFFIX))
        sync_directory(path.parent)
    return {'applied': True, 'changes': changes}


def main():
    parser = argparse.ArgumentParser(
        description='One-off ADR 0091 migration of a device\'s stored art.')
    parser.add_argument('--private', type=Path, required=True,
                        help="korrid's private state root, /var/lib/korrid on devices")
    parser.add_argument('--apply', action='store_true', help='write the changes; without it, a dry run')
    args = parser.parse_args()
    try:
        report = run(args.private, args.apply)
    except Refused as refusal:
        print(f'refused, nothing changed: {refusal}', file=sys.stderr)
        sys.exit(1)
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()

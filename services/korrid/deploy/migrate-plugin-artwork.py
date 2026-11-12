#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
"""One-off ADR 0032 migration of a device's artwork state. Not korrid code.

korrid no longer has a built-in SteamGridDB fetcher. Run this once on each
device when that korrid is deployed, with korrid stopped, as root or as the
user that owns korrid's private state root (`/var/lib/korrid` on devices):

    systemctl stop korrid
    ./migrate-plugin-artwork.py --private /var/lib/korrid          # dry run
    ./migrate-plugin-artwork.py --private /var/lib/korrid --apply
    systemctl start korrid

What it changes, all under the private root:

1. `game-assets/assignments.json`: each key `<playable id>` becomes
   `<playable id>:tile`, and each `source` changes from
   `{provider: "steamgriddb", game_id, grid_id, sha256, width, height, format}`
   to `{provider: "@korri:steamgriddb", id: "<grid_id>", sha256, width,
   height, format}`. `game_id` goes. No `url` is written.
2. `steamgriddb.credential`: its trimmed content becomes the plugin secret
   `@korri:steamgriddb/credential` (folder mode 0700, file mode 0600, owned
   as the private root is). The old file is then deleted.
3. `steamgriddb-enrichment/`: deleted. It records past lookups under outcome
   names that no longer exist.

`game-assets/blobs/` does not change, so no asset id and no `coverAssetId`
changes. A device with none of these files needs nothing.

It is a dry run unless `--apply` is given. It checks everything before it
writes anything, keeps one backup of the old `assignments.json` beside it,
and refuses to run on a private root that is already migrated. It prints
what it changed. It never prints or copies the key anywhere else.

Check afterwards: korrid starts, the local games list reports the same
`coverAssetId` values as before, and the settings snapshot shows the
plugin's secret as `Configured` if the old key file existed.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import stat
import sys
import tempfile

PLUGIN_ID = '@korri:steamgriddb'
OLD_PROVIDER = 'steamgriddb'
OLD_KEY_FILE = 'steamgriddb.credential'
OLD_ATTEMPTS_FOLDER = 'steamgriddb-enrichment'
ASSIGNMENTS = Path('game-assets') / 'assignments.json'
BACKUP_SUFFIX = '.before-plugin-artwork'
OLD_SOURCE_FIELDS = {'provider', 'game_id', 'grid_id', 'sha256', 'width', 'height', 'format'}
NEW_SOURCE_FIELDS = {'provider', 'id', 'url', 'sha256', 'width', 'height', 'format'}
RECORD_FIELDS = {'asset_id', 'role', 'owner', 'source'}
ROLES = ('tile', 'poster', 'hero', 'logo')


class Refused(Exception):
    """The state is not one this migration may change. Nothing was written."""


def is_old(key, record):
    source = record.get('source')
    return (set(record) == RECORD_FIELDS and isinstance(source, dict)
            and set(source) == OLD_SOURCE_FIELDS and source['provider'] == OLD_PROVIDER
            and record['role'] == 'tile' and isinstance(record['owner'], dict)
            and key == record['owner'].get('playable_id')
            and isinstance(source['grid_id'], int) and not isinstance(source['grid_id'], bool))


def is_new(key, record):
    source = record.get('source')
    return (set(record) >= RECORD_FIELDS and isinstance(source, dict)
            and set(source) <= NEW_SOURCE_FIELDS and isinstance(record['owner'], dict)
            and record['role'] in ROLES
            and key == f"{record['owner'].get('playable_id')}:{record['role']}")


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
    migrated = {}
    for key, record in records.items():
        source = record['source']
        migrated[f'{key}:tile'] = {
            'asset_id': record['asset_id'],
            'role': 'tile',
            'owner': record['owner'],
            'source': {
                'provider': PLUGIN_ID,
                'id': str(source['grid_id']),
                'sha256': source['sha256'],
                'width': source['width'],
                'height': source['height'],
                'format': source['format'],
            },
        }
    return (json.dumps(migrated, indent=2, sort_keys=True) + '\n').encode(), len(old)


def regular_file(path):
    return path.exists() and not path.is_symlink() and path.is_file()


def write_owned(path, data, mode, owner):
    """Replace `path` with a new file of exactly this mode and owner."""
    fd, name = tempfile.mkstemp(prefix='.korri-artwork-', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as output:
            os.fchmod(output.fileno(), mode)
            if os.geteuid() == 0:
                os.fchown(output.fileno(), owner.st_uid, owner.st_gid)
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)
    directory = os.open(path.parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def run(private, apply=False):
    """Check everything, then change it when `apply`. Returns the report."""
    if private.is_symlink() or not private.is_dir():
        raise Refused('the private state root is not a directory')
    owner = private.stat()
    if os.geteuid() not in (0, owner.st_uid):
        raise Refused('run as root or as the owner of the private state root')

    assignments = private / ASSIGNMENTS
    old_key = private / OLD_KEY_FILE
    old_attempts = private / OLD_ATTEMPTS_FOLDER
    plugin_folder = private / PLUGIN_ID
    credential = plugin_folder / 'credential'
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
    has_key = old_key.exists() or old_key.is_symlink()
    if has_key and not regular_file(old_key):
        raise Refused(f'{OLD_KEY_FILE} is not a regular file')
    if has_key and (credential.exists() or credential.is_symlink()):
        raise Refused('the plugin already has a secret; this migration will not replace it')
    if plugin_folder.is_symlink() or (plugin_folder.exists() and not plugin_folder.is_dir()):
        raise Refused(f'{PLUGIN_ID} is not a directory')
    has_attempts = old_attempts.exists() or old_attempts.is_symlink()
    if has_attempts and (old_attempts.is_symlink() or not old_attempts.is_dir()):
        raise Refused(f'{OLD_ATTEMPTS_FOLDER} is not a directory')

    if not (rewritten or has_key or has_attempts):
        migrated = credential.exists() or bool(original and json.loads(original))
        if migrated:
            raise Refused('this private state root is already migrated')
        return {'applied': False, 'changes': [], 'note': 'nothing to migrate'}

    # The key is read once, trimmed as korrid trimmed it, and never printed.
    secret = old_key.read_bytes().strip() if has_key else b''
    changes = []
    if rewritten:
        changes.append(f'{ASSIGNMENTS}: {rewritten} record(s) rewritten as <playable id>:tile '
                       f'with source provider {PLUGIN_ID}; backup {backup.name}')
    if has_key and secret:
        changes.append(f'{OLD_KEY_FILE}: moved to {PLUGIN_ID}/credential (0700 folder, 0600 file)')
    elif has_key:
        changes.append(f'{OLD_KEY_FILE}: empty, deleted; the plugin has no secret')
    if has_attempts:
        changes.append(f'{OLD_ATTEMPTS_FOLDER}/: deleted')
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
        write_owned(assignments, updated, stat.S_IMODE(metadata.st_mode), metadata)
    if has_key:
        if secret:
            plugin_folder.mkdir(mode=0o700, exist_ok=True)
            os.chmod(plugin_folder, 0o700)
            if os.geteuid() == 0:
                os.chown(plugin_folder, owner.st_uid, owner.st_gid)
            write_owned(credential, secret, 0o600, owner)
        old_key.unlink()
    if has_attempts:
        shutil.rmtree(old_attempts)
    return {'applied': True, 'changes': changes}


def main():
    parser = argparse.ArgumentParser(
        description='One-off ADR 0032 migration of a device\'s artwork state.')
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

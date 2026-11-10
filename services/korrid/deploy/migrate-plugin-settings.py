#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml ])"
"""One-off ADR 0057 migration. Run on a build host against staged config files.

The two paths and place shapes come from config/snapshot.rs, config/place.rs
and launch_settings/store.rs. Select only contributions whose old settings
are plugin-native. This is not a runtime migration or a fallback reader.
No raw config values are printed. Dry-run by default; backups are exclusive.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import tempfile
import yaml


class UniqueLoader(yaml.SafeLoader):
    pass


def mapping(loader, node, deep=False):
    result = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=deep)
        if not isinstance(key, str) or key in result:
            raise ValueError("configuration has a non-string or duplicate key")
        result[key] = loader.construct_object(value_node, deep=deep)
    return result


UniqueLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, mapping)


def places(document):
    if isinstance(document.get('host'), dict):
        yield document['host']
    for system in document.get('systems', {}).values():
        yield system
    for game in document.get('library', {}).values():
        yield game
        yield from game.get('releases', [])
        yield from game.get('profiles', {}).values()


def migrate(data, runners, families, modifiers=frozenset(), services=frozenset()):
    try:
        document = yaml.load(data, Loader=UniqueLoader)
    except yaml.YAMLError:
        raise ValueError('configuration is not valid YAML; no files changed') from None
    if not isinstance(document, dict):
        raise ValueError('configuration must be an object')
    changes = 0
    for place in places(document):
        if not isinstance(place, dict):
            raise ValueError('place must be an object')
        for section, selected in [('runners', runners), ('families', families), ('modifiers', modifiers), ('services', services)]:
            for key, record in place.get(section, {}).items():
                if key not in selected or 'settings' not in record:
                    continue
                settings = record['settings']
                if not isinstance(settings, dict):
                    raise ValueError('settings must be an object')
                if 'plugin' in settings:
                    # Refuse mixed old/new records. Never guess which keys won.
                    if len(settings) != 1:
                        raise ValueError('selected settings already contain plugin and other sections')
                    continue
                if settings:
                    record['settings'] = {'plugin': settings}
                    changes += 1
    return (yaml.safe_dump(document, sort_keys=False).encode() if changes else data), changes


def atomic_write(path, data, metadata):
    fd, name = tempfile.mkstemp(prefix='.korri-settings-', dir=path.parent)
    try:
        with os.fdopen(fd, 'wb') as output:
            os.fchmod(output.fileno(), metadata.st_mode & 0o7777)
            if os.geteuid() == 0:
                os.fchown(output.fileno(), metadata.st_uid, metadata.st_gid)
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def run(root, runners, families, apply=False, modifiers=frozenset(), services=frozenset()):
    candidates = []
    for relative in ['device.yaml', 'catalog/library.yaml']:
        path = root / relative
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'{relative} must be a regular file')
        original = path.read_bytes()
        updated, count = migrate(original, runners, families, modifiers, services)
        candidates.append((path, path.stat(), original, updated, count))
    report = [{'file': str(p.relative_to(root)), 'places': count,
               'beforeSha256': hashlib.sha256(before).hexdigest(),
               'afterSha256': hashlib.sha256(after).hexdigest()}
              for p, _, before, after, count in candidates]
    if apply:
        # Validate both documents before writing either. Back up both first.
        for path, metadata, original, _, count in candidates:
            if not count:
                continue
            if path.read_bytes() != original:
                raise ValueError('configuration changed during migration')
            backup = path.with_name(path.name + '.before-shared-settings')
            fd = os.open(backup, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, 'wb') as output:
                os.fchmod(output.fileno(), metadata.st_mode & 0o7777)
                if os.geteuid() == 0:
                    os.fchown(output.fileno(), metadata.st_uid, metadata.st_gid)
                output.write(original)
                output.flush()
                os.fsync(output.fileno())
        written = []
        try:
            for path, metadata, original, updated, count in candidates:
                if count:
                    if path.read_bytes() != original:
                        raise ValueError('configuration changed during migration')
                    atomic_write(path, updated, metadata)
                    written.append((path, metadata, original, updated))
        except Exception:
            for path, metadata, original, updated in reversed(written):
                if path.read_bytes() == updated:
                    atomic_write(path, original, metadata)
            raise
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True, help='staged readable config root on the build host')
    parser.add_argument('--runner', action='append', default=[])
    parser.add_argument('--family', action='append', default=[])
    parser.add_argument('--modifier', action='append', default=[])
    parser.add_argument('--service', action='append', default=[])
    parser.add_argument('--apply', action='store_true')
    args = parser.parse_args()
    if not any([args.runner, args.family, args.modifier, args.service]):
        parser.error('select at least one runner, family, modifier or service explicitly')
    print(json.dumps(run(args.root, set(args.runner), set(args.family), args.apply,
                         set(args.modifier), set(args.service)), indent=2))


if __name__ == '__main__':
    main()

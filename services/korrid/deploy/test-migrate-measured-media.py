#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
"""Tests of the one-off ADR 0091 measured-media migration, on real files."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / 'migrate-measured-media.py'
FIXTURES = HERE.parent / 'tests' / 'fixtures' / 'measured-media-migration'
spec = importlib.util.spec_from_file_location('migration', SCRIPT)
migration = importlib.util.module_from_spec(spec)
spec.loader.exec_module(migration)

BEFORE = (FIXTURES / 'before.assignments.json').read_bytes()
AFTER = (FIXTURES / 'after.assignments.json').read_bytes()
PLUGIN = '@korri:steamgriddb'
OLD_ATTEMPTS = json.dumps({'attempts': {'sha256:aa': {
    'outcome': 'assigned', 'playable_id': 'wario-land-4', 'release_id': 'r',
    'release_fingerprint': 'f', 'rom_identity': 'i'}}}).encode()


class Device:
    """A private state root as a device on main has it."""

    def __init__(self, test, assignments=BEFORE, attempts=True):
        self.folder = tempfile.TemporaryDirectory()
        test.addCleanup(self.folder.cleanup)
        self.private = Path(self.folder.name) / 'korrid'
        self.private.mkdir(mode=0o700)
        self.assignments = self.private / 'game-assets' / 'assignments.json'
        self.blobs = self.private / 'game-assets' / 'blobs'
        self.plugin = self.private / PLUGIN
        self.attempts = self.plugin / 'attempts.json'
        self.attempts_backup = self.plugin / 'attempts.json.before-measured-media'
        self.backup = self.assignments.with_name('assignments.json.before-measured-media')
        if assignments is not None:
            self.blobs.mkdir(parents=True)
            self.assignments.write_bytes(assignments)
            os.chmod(self.assignments, 0o640)
            for record in json.loads(assignments).values():
                (self.blobs / record['asset_id']).write_bytes(b'image bytes')
        # The plugin's secret is in the same folder and must not move.
        self.plugin.mkdir(mode=0o700)
        (self.plugin / 'credential').write_text('not a real secret')
        if attempts:
            self.attempts.write_bytes(OLD_ATTEMPTS)
        (self.private / 'identity').mkdir(mode=0o700)
        (self.private / 'identity' / 'device.key').write_text('device key')

    def tree(self):
        return {str(path.relative_to(self.private)): path.read_bytes() if path.is_file() else None
                for path in sorted(self.private.rglob('*'))}


class MigrationTests(unittest.TestCase):
    def test_a_device_is_migrated_in_one_run(self):
        device = Device(self)
        blobs_before = {path.name: path.read_bytes() for path in device.blobs.iterdir()}

        report = migration.run(device.private, apply=True)

        self.assertTrue(report['applied'])
        self.assertEqual(len(report['changes']), 2)
        self.assertEqual(device.assignments.read_bytes(), AFTER)
        records = json.loads(AFTER)
        self.assertEqual(sorted(records), [
            'anguna:@korri:steamgriddb:picture:0',
            'wario-land-4:@korri:steamgriddb:logo:0',
            'wario-land-4:@korri:steamgriddb:picture:0',
            'wario-land-4:@korri:steamgriddb:picture:1',
            'wario-land-4:@korri:steamgriddb:picture:2',
        ])
        # tile, poster, hero become pictures 0, 1 and 2; logo stays logo.
        old = json.loads(BEFORE)
        for old_key, new_key in [
            ('wario-land-4:tile', 'wario-land-4:@korri:steamgriddb:picture:0'),
            ('wario-land-4:poster', 'wario-land-4:@korri:steamgriddb:picture:1'),
            ('wario-land-4:hero', 'wario-land-4:@korri:steamgriddb:picture:2'),
            ('wario-land-4:logo', 'wario-land-4:@korri:steamgriddb:logo:0'),
            ('anguna:tile', 'anguna:@korri:steamgriddb:picture:0'),
        ]:
            expected = {name: value for name, value in old[old_key].items() if name != 'role'}
            expected['kind'] = 'logo' if old[old_key]['role'] == 'logo' else 'picture'
            self.assertEqual(records[new_key], expected, new_key)
        # The colour stays where it was, and stays absent where it was.
        self.assertEqual(records['wario-land-4:@korri:steamgriddb:logo:0']['color'], '#f0c828')
        self.assertNotIn('color', records['wario-land-4:@korri:steamgriddb:picture:2'])
        # The blobs and the mode of the file are untouched.
        self.assertEqual({path.name: path.read_bytes() for path in device.blobs.iterdir()}, blobs_before)
        self.assertEqual(device.assignments.stat().st_mode & 0o777, 0o640)
        # One backup of the old records, and the old attempts set aside.
        self.assertEqual(device.backup.read_bytes(), BEFORE)
        self.assertFalse(device.attempts.exists())
        self.assertEqual(device.attempts_backup.read_bytes(), OLD_ATTEMPTS)
        # Other private state is not touched.
        self.assertEqual((device.plugin / 'credential').read_text(), 'not a real secret')
        self.assertEqual((device.private / 'identity' / 'device.key').read_text(), 'device key')

    def test_it_refuses_to_run_twice(self):
        device = Device(self)
        migration.run(device.private, apply=True)
        after = device.tree()
        for apply in (False, True):
            with self.assertRaisesRegex(migration.Refused, 'already migrated'):
                migration.run(device.private, apply=apply)
        self.assertEqual(device.tree(), after)

    def test_attempts_korrid_wrote_after_the_migration_are_not_set_aside(self):
        device = Device(self)
        migration.run(device.private, apply=True)
        device.attempts.write_text('{"attempts": {}}')
        after = device.tree()
        with self.assertRaisesRegex(migration.Refused, 'already exists'):
            migration.run(device.private, apply=True)
        self.assertEqual(device.tree(), after)

    def test_a_dry_run_reports_and_writes_nothing(self):
        device = Device(self)
        before = device.tree()
        report = migration.run(device.private)
        self.assertFalse(report['applied'])
        self.assertEqual(len(report['changes']), 2)
        self.assertEqual(device.tree(), before)

    def test_a_device_with_none_of_the_files_needs_nothing(self):
        device = Device(self, assignments=None, attempts=False)
        before = device.tree()
        for apply in (False, True):
            report = migration.run(device.private, apply=apply)
            self.assertEqual(report, {'applied': False, 'changes': [], 'note': 'nothing to migrate'})
        self.assertEqual(device.tree(), before)

    def test_each_part_is_migrated_alone(self):
        only_records = Device(self, attempts=False)
        self.assertEqual(len(migration.run(only_records.private, apply=True)['changes']), 1)
        self.assertEqual(only_records.assignments.read_bytes(), AFTER)

        only_attempts = Device(self, assignments=None)
        self.assertEqual(len(migration.run(only_attempts.private, apply=True)['changes']), 1)
        self.assertFalse(only_attempts.attempts.exists())
        self.assertTrue(only_attempts.attempts_backup.exists())

    def test_a_run_that_stopped_part_way_finishes_on_the_next_run(self):
        # The records were rewritten and backed up, then the run stopped.
        device = Device(self)
        device.backup.write_bytes(BEFORE)
        device.assignments.write_bytes(AFTER)
        report = migration.run(device.private, apply=True)
        self.assertEqual(len(report['changes']), 1)
        self.assertEqual(device.assignments.read_bytes(), AFTER)
        self.assertFalse(device.attempts.exists())
        # The backup was written and the rewrite was not: it runs again.
        device = Device(self)
        device.backup.write_bytes(BEFORE)
        migration.run(device.private, apply=True)
        self.assertEqual(device.assignments.read_bytes(), AFTER)

    def test_states_it_does_not_know_are_refused_before_any_write(self):
        mixed = json.loads(BEFORE)
        mixed.update(json.loads(AFTER))
        unknown_role = json.loads(BEFORE)
        unknown_role['wario-land-4:hero']['role'] = 'banner'
        wrong_key = {'other-id:tile': json.loads(BEFORE)['wario-land-4:tile']}
        extra_field = json.loads(BEFORE)
        extra_field['anguna:tile']['ratio'] = 1.0
        first_migration = {'wario-land-4': {
            'asset_id': 'a' * 64 + '.png', 'role': 'tile', 'owner': {'playable_id': 'wario-land-4'},
            'source': {'provider': 'steamgriddb', 'game_id': 1, 'grid_id': 2}}}
        for name, assignments in [
            ('mixed', json.dumps(mixed).encode()),
            ('unknown role', json.dumps(unknown_role).encode()),
            ('wrong key', json.dumps(wrong_key).encode()),
            ('extra field', json.dumps(extra_field).encode()),
            ('before the first artwork migration', json.dumps(first_migration).encode()),
            ('not json', b'{'),
            ('not an object', b'[]'),
        ]:
            with self.subTest(name):
                device = Device(self, assignments=None)
                device.assignments.parent.mkdir(parents=True)
                device.assignments.write_bytes(assignments)
                before = device.tree()
                with self.assertRaises(migration.Refused):
                    migration.run(device.private, apply=True)
                # Not the records, not the attempts.
                self.assertEqual(device.tree(), before)

    def test_links_and_other_backups_are_refused(self):
        device = Device(self, attempts=False)
        device.attempts.symlink_to(device.private / 'identity' / 'device.key')
        before = device.tree()
        with self.assertRaisesRegex(migration.Refused, 'not a regular file'):
            migration.run(device.private, apply=True)
        self.assertEqual(device.tree(), before)

        device = Device(self)
        device.backup.write_text('{}')
        before = device.tree()
        with self.assertRaisesRegex(migration.Refused, 'other content'):
            migration.run(device.private, apply=True)
        self.assertEqual(device.tree(), before)

    def test_the_command_prints_what_it_changed(self):
        device = Device(self)
        for arguments, code in [([], 0), (['--apply'], 0), (['--apply'], 1)]:
            result = subprocess.run(
                [sys.executable, str(SCRIPT), '--private', str(device.private), *arguments],
                capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, code, result.stderr)
        self.assertIn('refused, nothing changed', result.stderr)
        self.assertEqual(device.assignments.read_bytes(), AFTER)


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3
"""Tests of the one-off ADR 0032 artwork migration, on real files."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / 'migrate-plugin-artwork.py'
FIXTURES = HERE.parent / 'tests' / 'fixtures' / 'artwork-migration'
spec = importlib.util.spec_from_file_location('migration', SCRIPT)
migration = importlib.util.module_from_spec(spec)
spec.loader.exec_module(migration)

BEFORE = (FIXTURES / 'before.assignments.json').read_bytes()
AFTER = (FIXTURES / 'after.assignments.json').read_bytes()
SECRET = 'migration-test-not-a-real-secret'
PLUGIN = '@korri:steamgriddb'


def mode(path):
    return stat.S_IMODE(path.stat().st_mode)


class Device:
    """A private state root as a device on main has it."""

    def __init__(self, test, assignments=BEFORE, key=f'  {SECRET}\n', attempts=True):
        self.folder = tempfile.TemporaryDirectory()
        test.addCleanup(self.folder.cleanup)
        self.private = Path(self.folder.name) / 'korrid'
        self.private.mkdir(mode=0o700)
        self.assignments = self.private / 'game-assets' / 'assignments.json'
        self.blobs = self.private / 'game-assets' / 'blobs'
        self.old_key = self.private / 'steamgriddb.credential'
        self.old_attempts = self.private / 'steamgriddb-enrichment'
        self.credential = self.private / PLUGIN / 'credential'
        if assignments is not None:
            self.blobs.mkdir(parents=True)
            self.assignments.write_bytes(assignments)
            for record in json.loads(assignments).values():
                (self.blobs / record['asset_id']).write_bytes(b'image bytes')
        if key is not None:
            # main wrote the key with the unit's umask, not with a fixed mode.
            self.old_key.write_text(key)
            os.chmod(self.old_key, 0o644)
        if attempts:
            self.old_attempts.mkdir()
            (self.old_attempts / 'attempts.json').write_text('{"attempts": {}}')
        # Other private state the migration must not touch.
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
        self.assertEqual(len(report['changes']), 3)
        # Old key `<playable id>` is now `<playable id>:tile`, with the source
        # rewritten: provider is the plugin id, grid_id is `id`, game_id is gone.
        self.assertEqual(json.loads(device.assignments.read_bytes()), json.loads(AFTER))
        self.assertEqual(device.assignments.read_bytes(), AFTER)
        records = json.loads(device.assignments.read_bytes())
        self.assertEqual(sorted(records), ['anguna:tile', 'wario-land-4:tile'])
        source = records['wario-land-4:tile']['source']
        self.assertEqual(source['provider'], PLUGIN)
        self.assertEqual(source['id'], '123456')
        self.assertNotIn('game_id', source)
        self.assertNotIn('grid_id', source)
        self.assertNotIn('url', source)
        # The asset id, the owner and the blobs are untouched: same covers.
        old = json.loads(BEFORE)
        for key, record in old.items():
            self.assertEqual(records[f'{key}:tile']['asset_id'], record['asset_id'])
            self.assertEqual(records[f'{key}:tile']['owner'], record['owner'])
        self.assertEqual({path.name: path.read_bytes() for path in device.blobs.iterdir()}, blobs_before)
        # The key moved, trimmed, into the plugin's folder with fixed modes.
        self.assertFalse(device.old_key.exists())
        self.assertEqual(device.credential.read_text(), SECRET)
        self.assertEqual(mode(device.credential.parent), 0o700)
        self.assertEqual(mode(device.credential), 0o600)
        self.assertEqual(sorted(path.name for path in device.credential.parent.iterdir()), ['credential'])
        # Past lookups are gone; other private state is not.
        self.assertFalse(device.old_attempts.exists())
        self.assertEqual((device.private / 'identity' / 'device.key').read_text(), 'device key')
        # One backup of the old records, for the operator.
        backup = device.assignments.with_name('assignments.json.before-plugin-artwork')
        self.assertEqual(backup.read_bytes(), BEFORE)

    def test_it_refuses_to_run_twice(self):
        device = Device(self)
        migration.run(device.private, apply=True)
        after = device.tree()
        for apply in (False, True):
            with self.assertRaisesRegex(migration.Refused, 'already migrated'):
                migration.run(device.private, apply=apply)
        self.assertEqual(device.tree(), after)

    def test_a_dry_run_reports_and_writes_nothing(self):
        device = Device(self)
        before = device.tree()
        report = migration.run(device.private)
        self.assertFalse(report['applied'])
        self.assertEqual(len(report['changes']), 3)
        self.assertEqual(device.tree(), before)

    def test_a_device_with_none_of_the_files_needs_nothing(self):
        device = Device(self, assignments=None, key=None, attempts=False)
        before = device.tree()
        for apply in (False, True):
            report = migration.run(device.private, apply=apply)
            self.assertEqual(report, {'applied': False, 'changes': [], 'note': 'nothing to migrate'})
        self.assertEqual(device.tree(), before)

    def test_each_part_is_migrated_alone(self):
        only_key = Device(self, assignments=None, attempts=False)
        self.assertEqual(len(migration.run(only_key.private, apply=True)['changes']), 1)
        self.assertEqual(only_key.credential.read_text(), SECRET)
        self.assertFalse(only_key.old_key.exists())

        only_records = Device(self, key=None, attempts=False)
        self.assertEqual(len(migration.run(only_records.private, apply=True)['changes']), 1)
        self.assertEqual(only_records.assignments.read_bytes(), AFTER)
        self.assertFalse(only_records.credential.exists())
        self.assertFalse(only_records.credential.parent.exists())

        only_attempts = Device(self, assignments=None, key=None)
        self.assertEqual(len(migration.run(only_attempts.private, apply=True)['changes']), 1)
        self.assertFalse(only_attempts.old_attempts.exists())

    def test_an_empty_key_file_leaves_the_plugin_without_a_secret(self):
        device = Device(self, key=' \n')
        report = migration.run(device.private, apply=True)
        self.assertIn('empty', ' '.join(report['changes']))
        self.assertFalse(device.old_key.exists())
        self.assertFalse(device.credential.exists())

    def test_a_run_that_stopped_part_way_finishes_on_the_next_run(self):
        device = Device(self)
        # The records were rewritten and backed up, then the run stopped.
        backup = device.assignments.with_name('assignments.json.before-plugin-artwork')
        backup.write_bytes(BEFORE)
        device.assignments.write_bytes(AFTER)
        report = migration.run(device.private, apply=True)
        self.assertEqual(len(report['changes']), 2)
        self.assertEqual(device.assignments.read_bytes(), AFTER)
        self.assertEqual(device.credential.read_text(), SECRET)
        self.assertFalse(device.old_attempts.exists())
        # The backup was written and the rewrite was not: it runs again.
        device = Device(self)
        device.assignments.with_name('assignments.json.before-plugin-artwork').write_bytes(BEFORE)
        migration.run(device.private, apply=True)
        self.assertEqual(device.assignments.read_bytes(), AFTER)

    def test_states_it_does_not_know_are_refused_before_any_write(self):
        mixed = json.loads(BEFORE)
        mixed.update(json.loads(AFTER))
        unknown_role = json.loads(BEFORE)
        unknown_role['wario-land-4']['role'] = 'hero'
        wrong_key = {'other-id': json.loads(BEFORE)['wario-land-4']}
        extra_field = json.loads(BEFORE)
        extra_field['anguna']['source']['url'] = 'https://example.org/a.png'
        other_provider = json.loads(BEFORE)
        other_provider['anguna']['source']['provider'] = 'manual'
        for name, assignments in [
            ('mixed', json.dumps(mixed).encode()),
            ('unknown role', json.dumps(unknown_role).encode()),
            ('wrong key', json.dumps(wrong_key).encode()),
            ('extra field', json.dumps(extra_field).encode()),
            ('other provider', json.dumps(other_provider).encode()),
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
                # Not the records, not the key, not the attempts.
                self.assertEqual(device.tree(), before)

    def test_a_secret_the_plugin_already_has_is_never_replaced(self):
        device = Device(self)
        device.credential.parent.mkdir(mode=0o700)
        device.credential.write_text('newer secret')
        before = device.tree()
        with self.assertRaisesRegex(migration.Refused, 'already has a secret'):
            migration.run(device.private, apply=True)
        self.assertEqual(device.tree(), before)

    def test_links_and_other_backups_are_refused(self):
        device = Device(self)
        device.old_key.unlink()
        device.old_key.symlink_to(device.private / 'identity' / 'device.key')
        with self.assertRaises(migration.Refused):
            migration.run(device.private, apply=True)
        self.assertEqual((device.private / 'identity' / 'device.key').read_text(), 'device key')

        device = Device(self)
        device.assignments.with_name('assignments.json.before-plugin-artwork').write_text('{}')
        before = device.tree()
        with self.assertRaisesRegex(migration.Refused, 'other content'):
            migration.run(device.private, apply=True)
        self.assertEqual(device.tree(), before)

    def test_the_command_prints_what_it_changed_and_never_the_key(self):
        device = Device(self)
        for arguments, code in [([], 0), (['--apply'], 0), (['--apply'], 1)]:
            result = subprocess.run(
                [sys.executable, str(SCRIPT), '--private', str(device.private), *arguments],
                capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, code, result.stderr)
            self.assertNotIn(SECRET, result.stdout + result.stderr)
        self.assertIn('refused, nothing changed', result.stderr)
        self.assertEqual(device.credential.read_text(), SECRET)
        # The report is the same data `run` returns, and holds no secret.
        captured = io.StringIO()
        with contextlib.redirect_stdout(captured):
            print(json.dumps(migration.run(Device(self).private)))
        self.assertNotIn(SECRET, captured.getvalue())
        self.assertIn('steamgriddb.credential: moved to @korri:steamgriddb/credential', captured.getvalue())


if __name__ == '__main__':
    unittest.main()

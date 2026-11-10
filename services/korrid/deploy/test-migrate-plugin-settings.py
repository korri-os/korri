#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml ])"
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('migration', Path(__file__).with_name('migrate-plugin-settings.py'))
migration = importlib.util.module_from_spec(spec)
spec.loader.exec_module(migration)


class MigrationTests(unittest.TestCase):
    def test_selective_move_retains_types_clears_and_other_places(self):
        original = b'host:\n  runners:\n    chosen: {settings: {video: {vsync: false, scale: null}}}\n    untouched: {settings: {video: {vsync: true}}}\n'
        output, count = migration.migrate(original, {'chosen'}, set())
        self.assertEqual(count, 1)
        data = migration.yaml.safe_load(output)
        chosen = data['host']['runners']['chosen']['settings']
        self.assertEqual(chosen, {'plugin': {'video': {'vsync': False, 'scale': None}}})
        self.assertEqual(data['host']['runners']['untouched']['settings'], {'video': {'vsync': True}})
        self.assertEqual(migration.migrate(output, {'chosen'}, set()), (output, 0))

    def test_all_layers_are_grounded_in_the_existing_place_structure(self):
        data = b'library:\n  game:\n    families: {ra: {settings: {rewind: {enable: true}}}}\n    releases: [{id: release, families: {ra: {settings: {video: {vsync: false}}}}}]\n    profiles: {quiet: {families: {ra: {settings: {audio: {volume: 25}}}}}}\n'
        _, count = migration.migrate(data, set(), {'ra'})
        self.assertEqual(count, 3)

    def test_rejects_ambiguous_or_duplicate_settings(self):
        for data in [b'host: {runners: {a: {settings: {plugin: {}, video: {vsync: true}}}}}', b'host: {}\nhost: {}\n']:
            with self.assertRaises(ValueError):
                migration.migrate(data, {'a'}, set())

    def test_dry_run_does_not_write_and_apply_backs_up_original_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'catalog').mkdir()
            before = b'# keep in backup\nhost: {runners: {a: {settings: {video: {vsync: true}}}}}\n'
            path = root / 'device.yaml'
            path.write_bytes(before)
            (root / 'catalog/library.yaml').write_bytes(b'library: {}\n')
            migration.run(root, {'a'}, set())
            self.assertEqual(path.read_bytes(), before)
            migration.run(root, {'a'}, set(), True)
            self.assertEqual(path.with_name('device.yaml.before-shared-settings').read_bytes(), before)
            self.assertIn(b'plugin:', path.read_bytes())
            self.assertEqual((root / 'catalog/library.yaml').read_bytes(), b'library: {}\n')


if __name__ == '__main__':
    unittest.main()

#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 python3Packages.pyyaml
"""Tests for the one-off ADR 0090 title-cleaning migration."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import tempfile
import unittest
from unittest import mock

import yaml

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("migration", HERE / "migrate.py")
migration = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(migration)

# The cases discovery/title.rs tests against, so both sides keep one rule.
SHARED = HERE.parent.parent / "test-data" / "title-cleaning.json"

EXPECTED = {
    "secret-of-mana-u": "Secret of Mana",
    "legend-of-zelda-the-oracle-of-ages-usa": "The Legend of Zelda - Oracle of Ages",
    "metroid-fusion": "Metroid Fusion",
    "castlevania": "Castlevania - Symphony of the Night",
    "final-fantasy-vii": "Final Fantasy VII",
    "pokemon-emerald": "Pokémon - Emerald",
}
UNCHANGED = {
    "super-mario-world": "My Mario",
    "wario-land-4": "Wario Land 4",
    "super-off-road": "Super_Off_Road_(U)",
    "air-delivery": "Air Delivery (v1.3)",
}


def quiet(_line):
    pass


class TitleRules(unittest.TestCase):
    def test_every_shared_case_matches_korrid(self):
        cases = json.loads(SHARED.read_text())["cases"]
        self.assertGreaterEqual(len(cases), 3)
        for raw, clean in cases:
            with self.subTest(raw=raw):
                self.assertEqual(migration.clean_title(raw), clean)

    def test_old_fallback_is_mains_rule(self):
        self.assertEqual(
            migration.old_fallback_title("Pokémon - Emerald (Rev 1)"), "Pokémon Emerald"
        )
        self.assertEqual(
            migration.old_fallback_title("Wario_Land_4 (USA) [!]"), "Wario Land 4"
        )


class Migration(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        (self.root / "catalog").mkdir()
        shutil.copyfile(HERE / "fixtures/device.yaml", self.root / "device.yaml")
        shutil.copyfile(
            HERE / "fixtures/library.before.yaml", self.root / "catalog/library.yaml"
        )
        self.library = self.root / "catalog/library.yaml"
        self.device = self.root / "device.yaml"
        self.running = mock.patch.object(migration, "korrid_running", return_value=False)
        self.running.start()

    def tearDown(self):
        self.running.stop()
        self.directory.cleanup()

    def titles(self):
        games = yaml.safe_load(self.library.read_text())["library"]
        return {game_id: game["title"] for game_id, game in games.items()}

    def backups(self):
        return sorted(self.root.glob("title-cleaning-backup-*"))

    def test_dry_run_prints_the_changes_and_writes_nothing(self):
        before = self.library.read_bytes()
        lines = []
        changes, folder = migration.migrate(self.root, dry_run=True, report=lines.append)
        self.assertIsNone(folder)
        self.assertEqual({k: new for k, (_, new, _) in changes.items()}, EXPECTED)
        self.assertEqual(self.library.read_bytes(), before)
        self.assertEqual(self.backups(), [])
        self.assertIn(
            "secret-of-mana-u: 'Secret of Mana (U) [!]' -> 'Secret of Mana' "
            "(from 'Secret of Mana (U) [!].sfc')",
            lines,
        )
        self.assertIn(
            "super-mario-world: kept 'My Mario': discovery did not write it", lines
        )

    def test_a_run_backs_up_byte_for_byte_then_cleans_only_discovered_titles(self):
        before = self.library.read_bytes()
        device = self.device.read_bytes()
        changes, folder = migration.migrate(self.root, report=quiet)
        self.assertEqual(len(changes), len(EXPECTED))
        self.assertEqual(self.backups(), [folder])
        self.assertEqual((folder / "catalog/library.yaml").read_bytes(), before)
        self.assertEqual(stat.S_IMODE(folder.stat().st_mode), 0o700)
        titles = self.titles()
        for game_id, title in {**EXPECTED, **UNCHANGED}.items():
            self.assertEqual(titles[game_id], title, game_id)
        self.assertEqual(self.device.read_bytes(), device)
        # Only titles changed; every other value reads back the same.
        old = migration.load(before.decode())
        new = migration.load(self.library.read_text())
        for game_id in EXPECTED:
            old["library"][game_id]["title"] = EXPECTED[game_id]
        self.assertEqual(new, old)
        self.assertEqual(new["library"]["wario-land-4"]["display"]["flag"], "yes")

    def test_a_second_run_changes_nothing_and_makes_no_backup(self):
        migration.migrate(self.root, report=quiet)
        after = self.library.read_bytes()
        changes, folder = migration.migrate(self.root, report=quiet)
        self.assertEqual((changes, folder), ({}, None))
        self.assertEqual(self.library.read_bytes(), after)
        self.assertEqual(len(self.backups()), 1)

    def test_file_set_titles_come_from_the_descriptor_or_the_first_disc(self):
        changes, _ = migration.migrate(self.root, dry_run=True, report=quiet)
        self.assertEqual(
            changes["castlevania"][2], "Castlevania - Symphony of the Night (USA).cue"
        )
        self.assertEqual(
            changes["final-fantasy-vii"][2], "Final Fantasy VII (USA) (Disc 1).chd"
        )

    def test_the_file_keeps_its_mode(self):
        os.chmod(self.library, 0o640)
        migration.migrate(self.root, report=quiet)
        self.assertEqual(stat.S_IMODE(self.library.stat().st_mode), 0o640)
        self.assertEqual(list(self.root.glob("catalog/.title-cleaning-*")), [])

    def test_a_backup_that_does_not_match_stops_before_any_write(self):
        before = self.library.read_bytes()
        real = migration.write_copy

        def corrupt(fd, data):
            real(fd, data[:-1])

        with mock.patch.object(migration, "write_copy", corrupt):
            with self.assertRaisesRegex(migration.MigrationError, "does not match"):
                migration.migrate(self.root, report=quiet)
        self.assertEqual(self.library.read_bytes(), before)

    def test_refuses_while_korrid_runs(self):
        before = self.library.read_bytes()
        with mock.patch.object(migration, "korrid_running", return_value=True):
            with self.assertRaisesRegex(migration.MigrationError, "stop korrid"):
                migration.migrate(self.root, report=quiet)
        self.assertEqual(self.library.read_bytes(), before)

    def test_refuses_duplicate_keys(self):
        self.library.write_text("library: {}\nlibrary: {}\n")
        with self.assertRaisesRegex(migration.MigrationError, "duplicate"):
            migration.migrate(self.root, dry_run=True, report=quiet)


if __name__ == "__main__":
    unittest.main()

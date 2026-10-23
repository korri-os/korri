import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

import yaml

HERE = Path(__file__).parent
SPEC = importlib.util.spec_from_file_location("migration", HERE / "migrate.py")
migration = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(migration)


class MigrationTests(unittest.TestCase):
    def fixture(self, root):
        (root / "catalog").mkdir()
        shutil.copyfile(HERE / "fixtures/device.before.yaml", root / "device.yaml")
        shutil.copyfile(
            HERE / "fixtures/library.before.yaml", root / "catalog/library.yaml"
        )

    def test_fixture_files_migrate_once_keep_plugin_keys_and_preserve_backups(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            before = [
                (root / name).read_bytes()
                for name in ("device.yaml", "catalog/library.yaml")
            ]
            changed, backup = migration.migrate(root, dry_run=True)
            self.assertTrue(changed)
            self.assertIsNone(backup)
            self.assertEqual(list(root.glob("launch-settings-backup-*")), [])
            self.assertEqual((root / "device.yaml").read_bytes(), before[0])
            changed, backup = migration.migrate(root)
            self.assertTrue(changed)
            self.assertEqual((backup / "device.yaml").read_bytes(), before[0])
            self.assertEqual((backup / "catalog/library.yaml").read_bytes(), before[1])
            device = yaml.safe_load((root / "device.yaml").read_text())
            library = yaml.safe_load((root / "catalog/library.yaml").read_text())
            self.assertEqual(
                device,
                yaml.load(
                    (HERE / "fixtures/device.after.yaml").read_text(),
                    Loader=migration.UniqueLoader,
                ),
            )
            self.assertEqual(
                library,
                yaml.load(
                    (HERE / "fixtures/library.after.yaml").read_text(),
                    Loader=migration.UniqueLoader,
                ),
            )
            self.assertEqual(device["host"]["preferences"], {"player-count": 4})
            self.assertEqual(device["host"]["args"], {"append": ["--device"]})
            self.assertEqual(
                device["host"]["runners"]["@korri:example/main"]["settings"],
                {"pluginCamelCase": True},
            )
            self.assertEqual(device["host"]["env"]["REMOVE"], None)
            self.assertNotIn("trust-removable", device["host"]["hooks"])
            self.assertIn("prepare", device["routines"])
            self.assertEqual(
                device["provider-links"]["link"]["refs"][0]["target-part"], "disc"
            )
            self.assertEqual(
                library["library"]["game"]["releases"][0]["args"]["append"],
                ["--release"],
            )
            self.assertEqual(migration.migrate(root), (False, None))
            self.assertEqual(len(list(root.glob("launch-settings-backup-*"))), 1)

    def test_unknown_shapes_and_key_clashes_refuse_without_writes(self):
        fixtures = [
            ({"runners": {}, "host": {"runners": {}}}, {}),
            ({"hooks": {}, "routines": {}}, {}),
            ({"host": {"argsAppend": [], "args": {"append": []}}}, {}),
            ({"host": {"preferences": {"playerCount": 2, "player-count": 3}}}, {}),
            ({"host": {"typo": True}}, {}),
            ({"host": {"runners": {"runner": {"runner": "not-allowed"}}}}, {}),
            ({"routines": {"routine": {"use": ["recursive"]}}}, {}),
            ({"host": {"argsAppend": "not-a-list"}}, {}),
            ({"host": {"env": {"X": 1}}}, {}),
            ({"host": {"cpu": {"min": "bad"}}}, {}),
            ({"host": {"cpu": {"min": 80, "max": 20}}}, {}),
            ({"systems": {"gba": {"cpu": {"governor": "performance"}}}}, {}),
            ({"host": {"runners": {"runner": {"settings": {"key": []}}}}}, {}),
            ({"routines": {"r": {"before": [{"run": "true", "timeout": 0}]}}}, {}),
            ({}, {"library": {"game": {"releases": "not-a-list"}}}),
            ({}, {"unknown": {}}),
        ]
        for device, library in fixtures:
            with (
                self.subTest(device=device, library=library),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                (root / "catalog").mkdir()
                (root / "device.yaml").write_text(yaml.safe_dump(device))
                (root / "catalog/library.yaml").write_text(yaml.safe_dump(library))
                before = (root / "device.yaml").read_bytes()
                with self.assertRaises(migration.MigrationError):
                    migration.migrate(root)
                self.assertEqual((root / "device.yaml").read_bytes(), before)
                self.assertEqual(list(root.glob("launch-settings-backup-*")), [])

    def test_loader_keeps_yaml12_plugin_names_and_values_unchanged(self):
        value = yaml.load(
            "host: {env: {ON: yes, OFF: no, DATE: 2026-10-22}, runners: {'@korri:example/main': {settings: {on: off, number: 0755, time: 1:20.0, exponent: 1e3}}}}",
            Loader=migration.UniqueLoader,
        )
        device, _ = migration.transform(value, {})
        self.assertEqual(
            device["host"]["env"], {"ON": "yes", "OFF": "no", "DATE": "2026-10-22"}
        )
        self.assertEqual(
            device["host"]["runners"]["@korri:example/main"]["settings"],
            {"on": "off", "number": 755, "time": "1:20.0", "exponent": 1000.0},
        )

    def test_refuses_while_an_actual_korrid_named_process_runs(self):
        process = subprocess.Popen(
            [
                sys.executable,
                "-c",
                "import ctypes,time; ctypes.CDLL(None).prctl(15,b'korrid',0,0,0); print('ready',flush=True); time.sleep(30)",
            ],
            stdout=subprocess.PIPE,
            text=True,
        )
        try:
            self.assertEqual(process.stdout.readline().strip(), "ready")
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.fixture(root)
                with self.assertRaisesRegex(migration.MigrationError, "stop korrid"):
                    migration.migrate(root)
                self.assertEqual(list(root.glob("launch-settings-backup-*")), [])
        finally:
            process.terminate()
            process.wait()
            process.stdout.close()

    def test_duplicate_keys_refuse(self):
        with self.assertRaises(migration.MigrationError):
            yaml.load("host: {}\nhost: {}", Loader=migration.UniqueLoader)

    def test_running_process_check_reads_real_proc_records(self):
        with tempfile.TemporaryDirectory() as directory:
            proc = Path(directory)
            (proc / "123").mkdir()
            (proc / "123/comm").write_text("other\n")
            self.assertFalse(migration.korrid_running(proc))
            (proc / "123/comm").write_text("korrid\n")
            self.assertTrue(migration.korrid_running(proc))

    def test_cli_dry_run_and_apply_use_the_real_fixture_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            command = [
                sys.executable,
                str(HERE / "migrate.py"),
                "--storage-root",
                str(root),
            ]
            self.assertEqual(
                subprocess.check_output(command + ["--dry-run"], text=True).strip(),
                "changes required",
            )
            self.assertIn("backup:", subprocess.check_output(command, text=True))
            self.assertEqual(
                subprocess.check_output(command, text=True).strip(), "already migrated"
            )


if __name__ == "__main__":
    unittest.main()

import importlib.util
import copy
import errno
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

import yaml

HERE = Path(__file__).parent
SPEC = importlib.util.spec_from_file_location("migration", HERE / "migrate.py")
migration = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(migration)


REFUSALS = [
    ("player-count-string.yaml", "host.preferences.playerCount", "four"),
    ("player-count-range.yaml", "host.preferences.player-count", 256),
    ("player-count-boolean.yaml", "host.preferences.playerCount", True),
    ("modifier-boolean.yaml", "modifiers.@korri:example/main.enabled", "true"),
    ("argument-list.yaml", "host.argsAppend", "--flag"),
    ("argument-member.yaml", "host.argsAppend[0]", False),
    ("argument-nul.yaml", "host.argsAppend[0]", "a\x00b"),
    ("settings-record.yaml", "families.@korri:example.settings", []),
    ("setting-scalar.yaml", "runners.@korri:example/main.settings.key", []),
    ("native-config-string.yaml", "runners.@korri:example/main.config.append", 42),
    ("hook-list.yaml", "routines.setup.before", "true"),
    ("hook-command.yaml", "hooks.setup.before[0].run", ""),
    ("hook-timeout.yaml", "routines.setup.before[0].timeout", 18446744073709551616),
    ("hook-policy.yaml", "routines.setup.before[0].on-failure", ["warn"]),
    ("hook-trust-boolean.yaml", "host.hooks.trust-removable", "yes"),
    ("cpu-range.yaml", "host.cpu.min", 101),
    ("gpu-range.yaml", "host.gpu.max", 0),
    ("environment-string.yaml", "host.env.VALUE", False),
    ("environment-name.yaml", "host.env", "BAD=NAME"),
    ("cwd-string.yaml", "host.cwd", []),
    ("family-id.yaml", "families.not-a-plugin", "not-a-plugin"),
    ("routine-reference.yaml", "host.hooks.use[0]", "missing"),
    (
        "setting-integer-above-u64.yaml",
        "runners.@korri:example/main.settings.large",
        18446744073709551616,
    ),
    (
        "setting-integer-below-i64.yaml",
        "runners.@korri:example/main.settings.large",
        -9223372036854775809,
    ),
    ("target-part-string.yaml", "provider-links.link.refs[0].targetPart", False),
]


class MigrationTests(unittest.TestCase):
    def fixture(self, root):
        (root / "catalog").mkdir()
        shutil.copyfile(HERE / "fixtures/device.before.yaml", root / "device.yaml")
        shutil.copyfile(
            HERE / "fixtures/library.before.yaml", root / "catalog/library.yaml"
        )

    def file_metadata(self, path):
        status = path.stat()
        return (
            status.st_uid,
            status.st_gid,
            stat.S_IMODE(status.st_mode),
            {name: os.getxattr(path, name) for name in os.listxattr(path)},
        )

    def set_source_metadata(self, paths, mode=0o2640):
        # Use a supplementary group when possible: a copy owned by the calling
        # user would otherwise hide the missing-chown bug in an unprivileged run.
        group = next(
            (gid for gid in os.getgroups() if gid != os.getegid()), os.getegid()
        )
        for path in paths:
            os.chown(path, -1, group)
            os.chmod(path, mode)

    def assert_user_xattrs_available(self, paths):
        for index, path in enumerate(paths):
            try:
                os.setxattr(
                    path, "user.korri-migration-test", f"original-{index}".encode()
                )
            except OSError as error:
                if error.errno in {errno.ENOTSUP, errno.EOPNOTSUPP, errno.ENOSYS}:
                    self.skipTest("filesystem does not support user xattrs")
                raise

    def set_acl(self, paths, *, default=False):
        utility = shutil.which("setfacl")
        if utility is None:
            self.skipTest("setfacl is unavailable; POSIX ACL test skipped")
        other_uid = 65534 if os.getuid() != 65534 else 65533
        spec = (
            f"u:{other_uid}:r--,m::r--"
            if not default
            else f"d:u::rwx,d:u:{other_uid}:r--,d:g::r-x,d:m::r-x,d:o::---"
        )
        for path in paths:
            result = subprocess.run(
                [utility, "-m", spec, str(path)],
                capture_output=True,
                text=True,
                env={**os.environ, "LC_ALL": "C"},
                check=False,
            )
            if result.returncode and any(
                reason in result.stderr.lower()
                for reason in ["not supported", "not implemented"]
            ):
                self.skipTest(
                    f"filesystem does not support POSIX ACLs: {result.stderr.strip()}"
                )
            self.assertEqual(result.returncode, 0, result.stderr)
            key = "system.posix_acl_default" if default else "system.posix_acl_access"
            try:
                os.getxattr(path, key)
            except OSError as error:
                if error.errno in {
                    errno.ENODATA,
                    errno.ENOTSUP,
                    errno.EOPNOTSUPP,
                    errno.ENOSYS,
                }:
                    self.skipTest("filesystem does not expose the POSIX ACL xattr")
                raise

    def test_replacements_and_backups_preserve_uid_gid_and_all_mode_bits(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            paths = [root / "device.yaml", root / "catalog/library.yaml"]
            self.set_source_metadata(paths)
            expected = [
                (
                    path.stat().st_uid,
                    path.stat().st_gid,
                    stat.S_IMODE(path.stat().st_mode),
                )
                for path in paths
            ]
            _, backup = migration.migrate(root)
            for path, metadata in zip(paths, expected):
                for candidate in [path, backup / path.relative_to(root)]:
                    actual = candidate.stat()
                    self.assertEqual(
                        (actual.st_uid, actual.st_gid, stat.S_IMODE(actual.st_mode)),
                        metadata,
                        str(candidate),
                    )

    def test_replacements_and_backups_preserve_xattrs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            paths = [root / "device.yaml", root / "catalog/library.yaml"]
            self.assert_user_xattrs_available(paths)
            self.set_source_metadata(paths)
            expected = [self.file_metadata(path) for path in paths]
            _, backup = migration.migrate(root)
            for path, metadata in zip(paths, expected):
                self.assertEqual(self.file_metadata(path), metadata, str(path))
                self.assertEqual(
                    self.file_metadata(backup / path.relative_to(root)), metadata
                )

    def test_replacements_and_backups_preserve_posix_acl(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            paths = [root / "device.yaml", root / "catalog/library.yaml"]
            self.set_source_metadata(paths, 0o640)
            self.set_acl(paths)
            expected = [self.file_metadata(path) for path in paths]
            self.assertTrue(
                all("system.posix_acl_access" in item[3] for item in expected)
            )
            _, backup = migration.migrate(root)
            for path, metadata in zip(paths, expected):
                self.assertEqual(self.file_metadata(path), metadata, str(path))
                self.assertEqual(
                    self.file_metadata(backup / path.relative_to(root)), metadata
                )

    def test_read_only_sources_keep_metadata_on_replacements_and_backups(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            paths = [root / "device.yaml", root / "catalog/library.yaml"]
            self.assert_user_xattrs_available(paths)
            self.set_source_metadata(paths, 0o440)
            expected = [self.file_metadata(path) for path in paths]
            _, backup = migration.migrate(root)
            for path, metadata in zip(paths, expected):
                self.assertEqual(self.file_metadata(path), metadata, str(path))
                self.assertEqual(
                    self.file_metadata(backup / path.relative_to(root)), metadata
                )

    def test_destination_default_acls_do_not_add_permissions_to_replacements_or_backups(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            paths = [root / "device.yaml", root / "catalog/library.yaml"]
            self.assert_user_xattrs_available(paths)
            self.set_source_metadata(paths)
            expected = [self.file_metadata(path) for path in paths]
            self.assertTrue(
                all("system.posix_acl_access" not in item[3] for item in expected)
            )
            self.set_acl([root, root / "catalog"], default=True)
            _, backup = migration.migrate(root)
            for path, metadata in zip(paths, expected):
                self.assertEqual(self.file_metadata(path), metadata, str(path))
                self.assertEqual(
                    self.file_metadata(backup / path.relative_to(root)), metadata
                )

    def test_metadata_copy_refuses_a_real_unprivileged_chown_failure(self):
        if os.geteuid() == 0:
            self.skipTest("requires an unprivileged test user")
        # The real interpreter is root-owned in the Nix toolchain. No mocks or
        # privileged setup: an ordinary user cannot assign its owner to a file.
        source = Path(sys.executable).resolve()
        metadata = migration.read_file_metadata(source)
        if metadata.uid == os.geteuid():
            self.skipTest(
                "interpreter has the test user's uid; no foreign owner to test"
            )
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "unpublished.yaml"
            with destination.open("w+") as output:
                with self.assertRaisesRegex(
                    migration.MigrationError, "cannot preserve metadata"
                ):
                    migration.apply_file_metadata(
                        output.fileno(), metadata, source, destination
                    )
                self.assertEqual(os.fstat(output.fileno()).st_uid, os.geteuid())
            self.assertEqual(destination.read_bytes(), b"")

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

    def test_invalid_player_count_is_refused_before_any_backup_or_write(self):
        source = (HERE / "fixtures/refusals/player-count-string.yaml").read_text()
        device = yaml.load(source, Loader=migration.UniqueLoader)
        with self.assertRaisesRegex(
            migration.MigrationError, "host.preferences.playerCount.*four"
        ):
            migration.transform(device, {})
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "catalog").mkdir()
            (root / "device.yaml").write_text(source)
            (root / "catalog/library.yaml").write_text("{}\n")
            with self.assertRaises(migration.MigrationError):
                migration.migrate(root)
            self.assertEqual((root / "device.yaml").read_text(), source)
            self.assertEqual(list(root.glob("launch-settings-backup-*")), [])

    def test_refusal_fixtures_name_the_path_key_and_value_before_any_writes(self):
        library = yaml.load(
            (HERE / "fixtures/library.after.yaml").read_text(),
            Loader=migration.UniqueLoader,
        )
        for filename, path, value in REFUSALS:
            source = (HERE / "fixtures/refusals" / filename).read_text()
            device = yaml.load(source, Loader=migration.UniqueLoader)
            before = copy.deepcopy(device)
            with self.subTest(fixture=filename):
                with self.assertRaises(migration.MigrationError) as caught:
                    migration.transform(device, library)
                self.assertIn(path, str(caught.exception))
                self.assertIn(repr(value), str(caught.exception))
                self.assertEqual(device, before)
                with tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    (root / "catalog").mkdir()
                    (root / "device.yaml").write_text(source)
                    shutil.copyfile(
                        HERE / "fixtures/library.after.yaml",
                        root / "catalog/library.yaml",
                    )
                    original_device = (root / "device.yaml").read_bytes()
                    original_library = (root / "catalog/library.yaml").read_bytes()
                    for dry_run in [True, False]:
                        with self.assertRaises(migration.MigrationError):
                            migration.migrate(root, dry_run=dry_run)
                        self.assertEqual(
                            (root / "device.yaml").read_bytes(), original_device
                        )
                        self.assertEqual(
                            (root / "catalog/library.yaml").read_bytes(),
                            original_library,
                        )
                        self.assertEqual(
                            list(root.glob("launch-settings-backup-*")), []
                        )
                        self.assertEqual(list(root.rglob(".launch-settings-*")), [])

    def test_renamed_and_moved_values_check_all_boundary_types_and_ranges(self):
        for key in ["playerCount", "player-count"]:
            for value in [None, True, False, "four", 4.0, 0, -1, 256, 2**64]:
                with (
                    self.subTest(key=key, value=value),
                    self.assertRaises(migration.MigrationError),
                ):
                    migration.transform({"host": {"preferences": {key: value}}}, {})
            for value in [1, 255]:
                migrated, _ = migration.transform(
                    {"host": {"preferences": {key: value}}}, {}
                )
                self.assertEqual(migrated["host"]["preferences"]["player-count"], value)
                if os.environ.get("KORRI_CONFIG_CHECK"):
                    migration.check_with_korrid(
                        [migrated, {}], Path(os.environ["KORRI_CONFIG_CHECK"])
                    )
        for value in [None, False, 0, 1.0, "30", -1, 0, 2**64]:
            with (
                self.subTest(timeout=value),
                self.assertRaises(migration.MigrationError),
            ):
                migration.transform(
                    {
                        "hooks": {
                            "setup": {"before": [{"run": "true", "timeout": value}]}
                        }
                    },
                    {},
                )
        for value in [1, 2**64 - 1]:
            migrated, _ = migration.transform(
                {"hooks": {"setup": {"before": [{"run": "true", "timeout": value}]}}},
                {},
            )
            self.assertEqual(
                migrated["routines"]["setup"]["before"][0]["timeout"], value
            )
            if os.environ.get("KORRI_CONFIG_CHECK"):
                migration.check_with_korrid(
                    [migrated, {}], Path(os.environ["KORRI_CONFIG_CHECK"])
                )
        for section in ["families", "runners", "modifiers"]:
            with (
                self.subTest(section=section),
                self.assertRaises(migration.MigrationError),
            ):
                migration.transform({section: []}, {})
        for value in [None, False, 0, 1, [], {}, ""]:
            with (
                self.subTest(targetPart=value),
                self.assertRaises(migration.MigrationError),
            ):
                migration.transform(
                    {
                        "provider-links": {
                            "link": {
                                "provider": "@korri:example",
                                "playable": "game",
                                "refs": [
                                    {
                                        "kind": "provider-item-id",
                                        "value": "item",
                                        "targetPart": value,
                                    }
                                ],
                            }
                        }
                    },
                    {},
                )

    def test_moved_records_keep_accepted_booleans_settings_and_nullable_runner_config(
        self,
    ):
        for enabled in [True, False]:
            for config in [
                None,
                {"prepend": None, "append": "native", "replace": None},
            ]:
                source = {
                    "runners": {
                        "@korri:example/main": {
                            "config": config,
                            "settings": {
                                "boolean": enabled,
                                "number": 3.5,
                                "string": "native",
                            },
                            "argsAppend": ["", "--flag"],
                        }
                    },
                    "families": {
                        "@korri:example": {
                            "cpu": {"min": 0, "max": 100},
                            "env": {"UNSET": None, "SET": "value"},
                        }
                    },
                    "modifiers": {
                        "@korri:example/main": {
                            "enabled": enabled,
                            "settings": {"value": enabled},
                            "config": {"append": "native"},
                        }
                    },
                }
                device, _ = migration.transform(source, {})
                self.assertEqual(
                    device["host"]["runners"]["@korri:example/main"]["config"], config
                )
                self.assertIs(
                    device["host"]["modifiers"]["@korri:example/main"]["enabled"],
                    enabled,
                )
                if os.environ.get("KORRI_CONFIG_CHECK"):
                    migration.check_with_korrid(
                        [device, {}], Path(os.environ["KORRI_CONFIG_CHECK"])
                    )

    def test_integer_limits_apply_to_every_scalar_position_without_a_probe(self):
        for number in [18446744073709551616, -9223372036854775809]:
            cases = [
                (
                    {
                        "runners": {
                            "@korri:example/main": {"settings": {"large": number}}
                        }
                    },
                    "runners.@korri:example/main.settings.large",
                ),
                (
                    {"families": {"@korri:example": {"settings": {"large": number}}}},
                    "families.@korri:example.settings.large",
                ),
                (
                    {
                        "modifiers": {
                            "@korri:example/main": {"settings": {"large": number}}
                        }
                    },
                    "modifiers.@korri:example/main.settings.large",
                ),
                (
                    {"systems": {"gba": {"metadata": {"nested": [number]}}}},
                    "systems.gba.metadata.nested[0]",
                ),
            ]
            for source, path in cases:
                with self.subTest(number=number, path=path):
                    with self.assertRaises(migration.MigrationError) as caught:
                        migration.transform(source, {})
                    self.assertIn(path, str(caught.exception))
                    self.assertIn(str(number), str(caught.exception))
                    self.assertFalse(migration.setting_scalar(number))
            library = yaml.load(
                (HERE / "fixtures/library.after.yaml").read_text(),
                Loader=migration.UniqueLoader,
            )
            library["library"]["game"]["metadata"] = {"nested": [number]}
            with self.assertRaises(migration.MigrationError) as caught:
                migration.transform({}, library)
            self.assertIn("library.game.metadata.nested[0]", str(caught.exception))
            self.assertIn(str(number), str(caught.exception))

    def test_integer_boundaries_and_leading_zero_fixture_round_trip(self):
        source = (HERE / "fixtures/scalars.before.yaml").read_text()
        expected = yaml.load(
            (HERE / "fixtures/scalars.after.yaml").read_text(),
            Loader=migration.UniqueLoader,
        )
        input_document = yaml.load(source, Loader=migration.UniqueLoader)
        settings = input_document["runners"]["@korri:example/main"]["settings"]
        self.assertEqual(settings["leading-zero"], "0755")
        self.assertIs(type(settings["leading-zero"]), str)
        self.assertIs(type(settings["signed-min"]), int)
        self.assertIs(type(settings["unsigned-max"]), int)
        self.assertIs(type(settings["large-float"]), float)
        transformed, _ = migration.transform(input_document, {})
        self.assertEqual(transformed, expected)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "catalog").mkdir()
            (root / "device.yaml").write_text(source)
            (root / "catalog/library.yaml").write_text("{}\n")
            probe = (
                Path(os.environ["KORRI_CONFIG_CHECK"])
                if os.environ.get("KORRI_CONFIG_CHECK")
                else None
            )
            migration.migrate(root, config_check=probe)
            saved = yaml.load(
                (root / "device.yaml").read_text(), Loader=migration.UniqueLoader
            )
            self.assertEqual(saved, expected)
            self.assertEqual(
                saved["host"]["runners"]["@korri:example/main"]["settings"][
                    "leading-zero"
                ],
                "0755",
            )
            self.assertEqual(migration.migrate(root), (False, None))
            if os.environ.get("KORRI_CONFIG_CHECK"):
                probe = Path(os.environ["KORRI_CONFIG_CHECK"])
                migration.check_with_korrid([saved, {}], probe)
                result = subprocess.run(
                    [
                        str(probe),
                        "--check",
                        str(root / "device.yaml"),
                        str(root / "catalog/library.yaml"),
                    ],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_guard_failures_refuse_before_backup_or_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            paths = [root / "device.yaml", root / "catalog/library.yaml"]
            before = [path.read_bytes() for path in paths]
            # A real rejecting program and a missing executable exercise both
            # external guard failures without replacing subprocess or file I/O.
            for probe in [Path("/bin/false"), root / "missing-probe"]:
                with (
                    self.subTest(probe=probe),
                    self.assertRaises(migration.MigrationError),
                ):
                    migration.migrate(root, config_check=probe)
                self.assertEqual([path.read_bytes() for path in paths], before)
                self.assertEqual(list(root.glob("launch-settings-backup-*")), [])
                self.assertEqual(list(root.rglob(".launch-settings-*")), [])

    @unittest.skipUnless(
        os.environ.get("KORRI_CONFIG_CHECK"),
        "set KORRI_CONFIG_CHECK to the prebuilt probe for decoder parity checks",
    )
    def test_real_probe_validates_transformed_documents_without_source_writes(self):
        probe = Path(os.environ["KORRI_CONFIG_CHECK"])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            before = [
                (root / path).read_bytes()
                for path in ["device.yaml", "catalog/library.yaml"]
            ]
            self.assertEqual(
                migration.migrate(root, dry_run=True, config_check=probe), (True, None)
            )
            self.assertEqual(
                [
                    (root / path).read_bytes()
                    for path in ["device.yaml", "catalog/library.yaml"]
                ],
                before,
            )
            self.assertEqual(list(root.glob("launch-settings-backup-*")), [])
            command = [
                sys.executable,
                str(HERE / "migrate.py"),
                "--storage-root",
                str(root),
                "--config-check",
                str(probe),
            ]
            self.assertIn("backup:", subprocess.check_output(command, text=True))
            self.assertEqual(
                subprocess.check_output(command, text=True).strip(), "already migrated"
            )

    @unittest.skipUnless(
        os.environ.get("KORRI_CONFIG_CHECK"),
        "set KORRI_CONFIG_CHECK to the prebuilt probe for decoder parity checks",
    )
    def test_real_probe_refuses_invalid_fixture_documents_without_writes(self):
        probe = Path(os.environ["KORRI_CONFIG_CHECK"])
        library_path = HERE / "fixtures/library.after.yaml"
        library = yaml.load(library_path.read_text(), Loader=migration.UniqueLoader)
        for filename, _, _ in REFUSALS:
            path = HERE / "fixtures/refusals" / filename
            before = path.read_bytes()
            library_before = library_path.read_bytes()
            with self.subTest(fixture=filename):
                result = subprocess.run(
                    [str(probe), "--check", str(path), str(library_path)],
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("CONFIG SNAPSHOT FAILED", result.stderr)
                with self.assertRaisesRegex(
                    migration.MigrationError, "config-check refused"
                ):
                    migration.check_with_korrid(
                        [yaml.load(before, Loader=migration.UniqueLoader), library],
                        probe,
                    )
                self.assertEqual(path.read_bytes(), before)
                self.assertEqual(library_path.read_bytes(), library_before)

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
            {"on": "off", "number": "0755", "time": "1:20.0", "exponent": 1000.0},
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

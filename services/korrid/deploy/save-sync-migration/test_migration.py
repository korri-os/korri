#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p "python3.withPackages (p: [ p.pyyaml ])"
"""Exercise the actual operator commands, only on temporary fixture trees."""

import errno
import json
import os
import pwd
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).parent
OWNER = "ab" * 32
DEVICE = "cd" * 32
RUNNER = "@korri:mgba/mgba"
DAEMON_UID = os.getuid()
RUNTIME_UID = pwd.getpwnam(
    "nobody" if DAEMON_UID != pwd.getpwnam("nobody").pw_uid else "root"
).pw_uid


class MigrationTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="save-migration-", dir="/tmp")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.storage = self.root / "storage"
        self.private = self.root / "private"
        self.old = self.storage / "users/default"
        self.account = self.storage / "users" / OWNER
        self.old.mkdir(parents=True)
        (self.private / "identity").mkdir(parents=True)
        # identity.rs OWNER_EVENT_FILE: use only public fields, never a device key.
        self.event = {
            "pubkey": OWNER,
            "kind": 30078,
            "content": "",
            "tags": [
                ["d", f"org.korri.device-owner:{DEVICE}"],
                ["device", DEVICE],
                ["status", "owned"],
            ],
        }
        self.write_event()
        self.catalog({"wario-land-4": ["gba/Wario.gba"]})

    def write_event(self):
        (self.private / "identity/owner.event.json").write_text(json.dumps(self.event))

    def catalog(self, games):
        library, locations = {}, {}
        for index, (game, paths) in enumerate(games.items()):
            releases = []
            for number, path in enumerate(paths):
                identity = f"sha256:{index * 100 + number:064x}"
                releases.append(
                    {
                        "id": f"release-{number}",
                        "system": "gba",
                        "identity": {"kind": "hash", "value": identity},
                    }
                )
                locations[identity] = [{"storage": "roms", "path": path}]
            library[game] = {"releases": releases}
        (self.storage / "catalog").mkdir(exist_ok=True)
        # JSON is YAML; this is the actual library.rs / config::Location shape.
        (self.storage / "catalog/library.yaml").write_text(
            json.dumps({"library": library})
        )
        (self.storage / "device.yaml").write_text(json.dumps({"locations": locations}))

    def file(self, relative, data=b"saved progress"):
        path = self.old / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        path.chmod(0o640)
        return path

    def require_acl(self):
        probe = self.root / "acl-probe"
        probe.mkdir()
        try:
            os.setxattr(
                probe,
                "system.posix_acl_access",
                struct.pack("<I", 2)
                + b"".join(
                    struct.pack("<HHI", *entry)
                    for entry in [
                        (1, 7, 0xFFFFFFFF),
                        (2, 7, RUNTIME_UID),
                        (4, 0, 0xFFFFFFFF),
                        (16, 7, 0xFFFFFFFF),
                        (32, 0, 0xFFFFFFFF),
                    ]
                ),
            )
        except OSError as error:
            if error.errno in (errno.ENOTSUP, errno.EOPNOTSUPP, errno.ENOSYS):
                self.skipTest(f"Test filesystem lacks POSIX ACL support: {error}")
            raise
        finally:
            probe.rmdir()

    def command(self, script, *args, ok=True):
        entry = [str(HERE / script)]
        if script == "migrate-retroarch.py":
            if ok and "--dry-run" not in args:
                self.require_acl()
            # Exercise the real CLI parser and orchestration in a subprocess,
            # with numeric identities; developer hosts need no product users.
            entry = [
                "-c",
                "import sys; sys.dont_write_bytecode = True; "
                f"sys.path.insert(0, {str(HERE)!r}); "
                "from migration import run; "
                f"sys.exit(run('retroarch', ({DAEMON_UID}, {RUNTIME_UID})))",
            ]
        result = subprocess.run(
            [
                sys.executable,
                *entry,
                "--storage-root",
                str(self.storage),
                "--private-state-root",
                str(self.private),
                *args,
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode == 0, ok, result.stdout + result.stderr)
        return result

    def tree(self):
        return sorted(
            (
                str(p.relative_to(self.root)),
                p.lstat().st_mode,
                p.read_bytes() if p.is_file() and not p.is_symlink() else None,
                tuple(
                    (name, os.getxattr(p, name, follow_symlinks=False))
                    for name in sorted(os.listxattr(p, follow_symlinks=False))
                ),
            )
            for p in self.root.rglob("*")
        )

    def move_account(self):
        self.command("migrate-account.py")

    def test_account_dry_run_is_read_only_and_move_keeps_metadata(self):
        source = self.file("saves/Wario.srm")
        inode = source.stat().st_ino
        before = self.tree()
        result = self.command("migrate-account.py", "--dry-run")
        self.assertIn(str(self.account), result.stdout)
        self.assertEqual(before, self.tree())
        self.move_account()
        moved = self.account / "saves/Wario.srm"
        self.assertEqual(moved.stat().st_ino, inode)
        self.assertEqual(moved.stat().st_mode & 0o777, 0o640)
        self.assertFalse(self.old.exists())
        self.command("migrate-account.py")  # rerun after success

    def test_per_game_dry_run_and_real_moves_preserve_raw_runner_and_unmatched(self):
        paths = [
            "saves/Wario.srm",
            f"states/{RUNNER}/Wario.state.auto",
            f"states/{RUNNER}/Wario.state1.png",
            "screenshots/Wario-261021-120001.png",
            "screenshots/Wario.png",
        ]
        for path in paths:
            self.file(path)
        self.file("saves/Unknown.srm")
        self.file("screenshots/Wario-not-a-timestamp.png")
        self.move_account()
        before = self.tree()
        result = self.command("migrate-retroarch.py", "--dry-run")
        self.assertIn("UNMATCHED", result.stdout)
        self.assertEqual(before, self.tree())
        self.command("migrate-retroarch.py")
        for path in paths:
            destination = self.account / "saves/wario-land-4" / path
            self.assertEqual(destination.read_bytes(), b"saved progress")
            self.assertEqual(destination.stat().st_mode & 0o777, 0o660)
            self.assert_shared_acl(destination, 6)
            self.assert_shared_acl(destination.parent, 7, default=True)
            self.assertFalse((self.account / path).exists())
        self.assertTrue((self.account / "saves/Unknown.srm").exists())
        self.assertTrue(
            (self.account / "screenshots/Wario-not-a-timestamp.png").exists()
        )
        before = self.tree()
        self.command("migrate-retroarch.py")
        self.assertEqual(before, self.tree())

    def assert_shared_acl(
        self, path, permissions, *, default=False, named_permissions=None
    ):
        if named_permissions is None:
            named_permissions = permissions
        name = "system.posix_acl_default" if default else "system.posix_acl_access"
        acl = os.getxattr(path, name)
        self.assertEqual(struct.unpack_from("<I", acl)[0], 2)
        entries = {
            (tag, uid): access
            for tag, access, uid in struct.iter_unpack("<HHI", acl[4:])
        }
        self.assertEqual(
            entries,
            {
                (1, 0xFFFFFFFF): permissions,
                (2, DAEMON_UID): named_permissions,
                (2, RUNTIME_UID): named_permissions,
                (4, 0xFFFFFFFF): 0,
                (16, 0xFFFFFFFF): permissions,
                (32, 0xFFFFFFFF): 0,
            },
        )

    def test_existing_save_tree_is_repaired_without_moves_and_defaults_inherit(self):
        self.require_acl()
        self.file("saves/wario-land-4/saves/mGBA/Wario.srm")
        self.file("saves/wario-land-4/states/runner/Wario.state")
        executable = self.file("saves/wario-land-4/executable")
        executable.chmod(0o750)
        self.file("steam/credentials", b"private")
        self.move_account()
        saves = self.account / "saves"
        before = self.tree()
        dry_run = self.command("migrate-retroarch.py", "--dry-run")
        self.assertIn("ACL", dry_run.stdout)
        self.assertEqual(before, self.tree())
        self.assertNotIn("MOVE", dry_run.stdout)
        outside = [item for item in before if "/saves" not in item[0]]
        metadata = {
            path: (path.stat().st_ino, path.stat().st_uid, path.stat().st_gid)
            for path in saves.rglob("*")
        }
        self.command("migrate-retroarch.py")
        self.assertEqual(
            outside, [item for item in self.tree() if "/saves" not in item[0]]
        )
        for path in [saves, *saves.rglob("*")]:
            permissions = 7 if path.is_dir() or path.name == "executable" else 6
            self.assert_shared_acl(path, permissions)
            if path.is_dir():
                self.assert_shared_acl(path, 7, default=True)
            else:
                self.assertNotIn("system.posix_acl_default", os.listxattr(path))
        for path, expected in metadata.items():
            self.assertEqual(
                (path.stat().st_ino, path.stat().st_uid, path.stat().st_gid), expected
            )
        before = self.tree()
        self.command("migrate-retroarch.py")
        self.assertEqual(before, self.tree())
        inherited = saves / "wario-land-4/new-location"
        inherited.mkdir()
        self.assert_shared_acl(inherited, 7)
        self.assert_shared_acl(inherited, 7, default=True)
        new_save = inherited / "new.srm"
        new_save.write_bytes(b"new progress")
        self.assert_shared_acl(new_save, 6, named_permissions=7)

    def test_states_only_migration_grants_access_to_new_save_tree(self):
        self.file(f"states/{RUNNER}/Wario.state")
        self.move_account()
        self.assertFalse((self.account / "saves").exists())
        before = self.tree()
        result = self.command("migrate-retroarch.py", "--dry-run")
        self.assertIn("ACL", result.stdout)
        self.assertEqual(before, self.tree())
        self.command("migrate-retroarch.py")
        self.assert_shared_acl(self.account / "saves", 7, default=True)
        self.assert_shared_acl(
            self.account / f"saves/wario-land-4/states/{RUNNER}/Wario.state", 6
        )

    def test_acl_function_accepts_numeric_identities_and_dry_run_is_read_only(self):
        from migration import grant_save_access

        self.require_acl()
        self.file("saves/game/saves/progress.srm")
        root = self.old / "saves"
        before = self.tree()
        grant_save_access(root, DAEMON_UID, RUNTIME_UID, dry_run=True)
        self.assertEqual(before, self.tree())
        grant_save_access(root, DAEMON_UID, RUNTIME_UID)
        self.assert_shared_acl(root, 7, default=True)
        self.assert_shared_acl(root / "game/saves/progress.srm", 6)

    def test_nested_save_symlink_or_hardlink_refuses_before_any_move(self):
        self.file("saves/Wario.srm")
        secret = self.file("steam/credentials", b"private")
        self.move_account()
        nested = self.account / "saves/wario-land-4/nested"
        nested.mkdir(parents=True)
        link = nested / "credentials"
        for hardlink in (False, True):
            if hardlink:
                os.link(self.account / secret.relative_to(self.old), link)
            else:
                link.symlink_to(self.account / "steam", target_is_directory=True)
            before = self.tree()
            self.command("migrate-retroarch.py", "--dry-run", ok=False)
            self.command("migrate-retroarch.py", ok=False)
            self.assertEqual(before, self.tree())
            link.unlink()

    def test_real_entrypoint_reports_missing_product_user_without_moving_files(self):
        missing = []
        for name in ("korrid", "korri"):
            try:
                pwd.getpwnam(name)
            except KeyError:
                missing.append(name)
        if not missing:
            self.skipTest("Both product users exist on this host")
        self.file("saves/Wario.srm")
        self.move_account()
        before = self.tree()
        result = subprocess.run(
            [
                sys.executable,
                str(HERE / "migrate-retroarch.py"),
                "--storage-root",
                str(self.storage),
                "--private-state-root",
                str(self.private),
            ],
            text=True,
            capture_output=True,
            check=False,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn(missing[0], result.stderr)
        self.assertIn("Migration refused", result.stderr)
        self.assertEqual(before, self.tree())

    def test_both_commands_refuse_any_active_or_unreconciled_session(self):
        journal = self.private / "host-session/active.json"
        journal.parent.mkdir()
        for phase in ["running", "completionPending", "unknown"]:
            journal.write_text(json.dumps({"phase": phase}))
            before = self.tree()
            for script in ["migrate-account.py", "migrate-retroarch.py"]:
                for flags in [(), ("--dry-run",)]:
                    self.command(script, *flags, ok=False)
            self.assertEqual(before, self.tree())

    def test_owner_collision_refuses_without_merging(self):
        self.account.mkdir()
        before = self.tree()
        self.command("migrate-account.py", ok=False)
        self.assertEqual(before, self.tree())

    def test_per_game_collision_preflights_every_move(self):
        self.file("saves/Wario.srm")
        self.file(f"states/{RUNNER}/Wario.state")
        self.file(f"saves/wario-land-4/states/{RUNNER}/Wario.state", b"other progress")
        self.move_account()
        before = self.tree()
        self.command("migrate-retroarch.py", ok=False)
        self.assertEqual(before, self.tree())

    def test_core_sorted_saves_move_with_their_core_folder(self):
        # RetroArch's sort-by-core layout, as found on the RP Mini V2.
        self.file("saves/mGBA/Wario.srm")
        self.file("saves/mGBA/Unknown.srm")
        self.move_account()
        result = self.command("migrate-retroarch.py", "--dry-run")
        self.assertIn("UNMATCHED", result.stdout)
        self.command("migrate-retroarch.py")
        moved = self.account / "saves/wario-land-4/saves/mGBA/Wario.srm"
        self.assertEqual(moved.read_bytes(), b"saved progress")
        self.assertFalse((self.account / "saves/mGBA/Wario.srm").exists())
        self.assertTrue((self.account / "saves/mGBA/Unknown.srm").exists())
        before = self.tree()
        self.command("migrate-retroarch.py")  # per-game roots are never rescanned
        self.assertEqual(before, self.tree())

    def test_ambiguous_stems_stay_but_multiple_releases_of_one_game_move(self):
        self.catalog(
            {
                "wario-land-4": [
                    "gba/Wario.gba",
                    "other/Wario.gba",
                    "gba/Wario USA.gba",
                ],
                "different-game": ["other/Wario USA.gba"],
            }
        )
        self.file("saves/Wario.srm")
        self.file("saves/Wario USA.srm")
        self.move_account()
        result = self.command("migrate-retroarch.py")
        self.assertIn("AMBIGUOUS", result.stdout)
        self.assertTrue((self.account / "saves/wario-land-4/saves/Wario.srm").exists())
        self.assertTrue((self.account / "saves/Wario USA.srm").exists())

    def test_symlink_source_or_destination_refuses(self):
        self.file("saves/Wario.srm")
        self.move_account()
        target = self.root / "elsewhere"
        target.mkdir()
        (self.account / "saves/wario-land-4").symlink_to(
            target, target_is_directory=True
        )
        before = self.tree()
        self.command("migrate-retroarch.py", ok=False)
        self.assertEqual(before, self.tree())
        (self.account / "saves/wario-land-4").unlink()
        (self.account / "saves/Wario.srm").unlink()
        (self.account / "saves/Wario.srm").symlink_to(target)
        self.command("migrate-retroarch.py", ok=False)
        self.assertEqual(list(target.iterdir()), [])

    def test_missing_private_root_does_not_bypass_session_guard(self):
        self.command(
            "migrate-account.py",
            "--private-state-root",
            str(self.root / "absent"),
            ok=False,
        )
        self.assertFalse((self.root / "absent").exists())

    def test_missing_revoked_or_malformed_owner_refuses(self):
        for public_key, status in [("../escape", "owned"), (OWNER, "revoked")]:
            self.event["pubkey"] = public_key
            self.event["tags"][2][1] = status
            self.write_event()
            self.command("migrate-account.py", ok=False)
        (self.private / "identity/owner.event.json").unlink()
        self.command("migrate-account.py", ok=False)

    def test_duplicate_yaml_keys_and_unsafe_game_ids_refuse(self):
        self.file("saves/Wario.srm")
        self.move_account()
        self.catalog({"..": ["Wario.gba"]})
        self.command("migrate-retroarch.py", ok=False)
        (self.storage / "catalog/library.yaml").write_text("library: {}\nlibrary: {}\n")
        self.command("migrate-retroarch.py", ok=False)

    def test_path_codec_matches_play_log_vectors(self):
        from migration import encode_component

        for value, expected in [
            ("abc-_.!~*'()", "abc-_.!~*'()"),
            ("steam/1029210", "steam%2F1029210"),
            ("a b&c=d", "a%20b%26c%3Dd"),
            ("../x", "..%2Fx"),
            ("é", "%C3%A9"),
            ("@korri:steam", "%40korri%3Asteam"),
        ]:
            self.assertEqual(encode_component(value), expected)


if __name__ == "__main__":
    unittest.main()

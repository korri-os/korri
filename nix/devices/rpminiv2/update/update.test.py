#!/usr/bin/env python3
"""Offline contracts: real files, shell transaction and Nix profiles; no SSH."""

import importlib.util
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("update", HERE / "run.py")
update = importlib.util.module_from_spec(spec)
spec.loader.exec_module(update)
OLD = "/nix/store/" + "a" * 32 + "-nixos-system-rpminiv2-sd-card-old"
NEW = "/nix/store/" + "b" * 32 + "-nixos-system-rpminiv2-sd-card-new"


def menu(system, title="NixOS Retroid Pocket Mini V2 Korri"):
    # The production renderer is esp.nix. This is just a parser input fixture.
    return f"""menuentry '{title}' {{
        search --set -f /EFI/nixos/{"c" * 32}-Image
        linux /EFI/nixos/{"c" * 32}-Image init={system}/init quiet
        initrd /EFI/nixos/{"d" * 32}-initrd
        devicetree /EFI/nixos/{"c" * 32}-board.dtb
}}
"""


class GrubTests(unittest.TestCase):
    def test_new_first_previous_second_and_no_older_entries(self):
        result = update.updated_grub(
            "set default=0\n" + menu(NEW),
            "set default=0\n" + menu(OLD) + menu(NEW, "older"),
            OLD,
        )
        entries = update.parse_grub(result)[1]
        self.assertEqual([entry["system"] for entry in entries], [NEW, OLD])
        self.assertIn("(previous)", entries[1]["text"])

    @unittest.skipUnless(
        os.environ.get("RP_MINIV2_TEST_FIRMWARE"),
        "run via rpminiv2-update-check for Nix renderer test",
    )
    def test_production_renderer_and_archive_for_two_toplevels(self):
        root = Path(os.environ["RP_MINIV2_TEST_FIRMWARE"])
        old = (root / "old/boot/grub/grub.cfg").read_text()
        new = (root / "new/boot/grub/grub.cfg").read_text()
        previous = update.parse_grub(old)[1][0]["system"]
        current = update.parse_grub(new)[1][0]["system"]
        grub = update.updated_grub(new, old, previous)
        self.assertEqual(
            [entry["system"] for entry in update.parse_grub(grub)[1]],
            [current, previous],
        )
        payload, required = update.archive_payload(root / "new", grub)
        self.assertGreater(required, len(payload))
        import io
        import tarfile

        with tarfile.open(fileobj=io.BytesIO(payload)) as archive:
            self.assertTrue(all(member.isfile() for member in archive.getmembers()))
            self.assertEqual(archive.extractfile("grub.cfg").read(), grub.encode())

    def test_refuses_wrong_default_and_changed_handoff(self):
        with self.assertRaises(ValueError):
            update.updated_grub("set default=0\n" + menu(NEW), menu(OLD), OLD)
        with self.assertRaises(ValueError):
            update.updated_grub(menu(NEW), menu(NEW) + menu(OLD), OLD)

    def test_refuses_unsafe_or_incomplete_payloads(self):
        for text in (
            menu(NEW).replace("/EFI/nixos/", "/EFI/nixos/../"),
            menu(NEW).replace("        initrd", "        unknown"),
            menu(NEW).replace(" quiet", " quiet init=/bad/init"),
        ):
            with self.subTest(text=text), self.assertRaises(ValueError):
                update.parse_grub(text)


class TargetTests(unittest.TestCase):
    def shell(self, script, check=True):
        return subprocess.run(
            [
                "bash",
                "-c",
                f"set -euo pipefail; source {shlex.quote(str(HERE / 'target.sh'))};\n"
                + script,
            ],
            text=True,
            capture_output=True,
            check=check,
        )

    def test_mount_refusals(self):
        for target, source, kind, label in [
            ("/boot", "/dev/sda21", "vfat", "RPMINIV2"),
            ("/", "/dev/sda22", "ext4", "NIXOS_RPMINIV2"),
        ]:
            args = [target, source, kind, label, "rw,relatime"]
            self.shell("check_mount_values " + shlex.join(args))
            for index, replacement in [
                (1, "/dev/mmcblk0p1"),
                (1, "/dev/sdb21"),
                (1, source + "[/bind]"),
                (2, "tmpfs"),
                (3, "WRONG"),
                (4, "ro,relatime"),
            ]:
                wrong = args.copy()
                wrong[index] = replacement
                with self.subTest(wrong=wrong):
                    result = self.shell(
                        "check_mount_values " + shlex.join(wrong), check=False
                    )
                    self.assertNotEqual(result.returncode, 0)

    def test_atomic_update_and_exact_rollback_with_real_nix_profiles(self):
        old = str(Path(shutil.which("bash")).resolve().parents[1])
        new = str(Path(shutil.which("python3")).resolve().parents[1])
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            grub = root / "boot/boot/grub/grub.cfg"
            grub.parent.mkdir(parents=True)
            original = b"# exact old bytes\n\nset default=0\n" + menu(old).encode()
            grub.write_bytes(original)
            profile = root / "profiles/system"
            profile.parent.mkdir()
            candidate = root / "candidate.cfg"
            candidate.write_text("set default=0\n" + menu(NEW) + menu(OLD))
            script = f"""
                nix-env -p {shlex.quote(str(profile))} --set {shlex.quote(old)}
                commit_update {shlex.quote(str(grub))} {shlex.quote(str(profile))} {shlex.quote(str(candidate))} {shlex.quote(new)}
                test "$(readlink -f {shlex.quote(str(profile))})" = {shlex.quote(new)}
                test "$(readlink -f {shlex.quote(str(profile))}-rpminiv2-previous)" = {shlex.quote(old)}
                rollback {shlex.quote(str(grub))} {shlex.quote(str(profile))}
                test "$(readlink -f {shlex.quote(str(profile))})" = {shlex.quote(old)}
            """
            self.shell(script)
            self.assertEqual(grub.read_bytes(), original)
            self.assertEqual(Path(str(grub) + ".previous").read_bytes(), original)

    def test_existing_payload_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temp:
            source, dest = Path(temp) / "source", Path(temp) / "dest"
            source.write_bytes(b"new")
            dest.write_bytes(b"old")
            result = self.shell(
                "copy_payload " + shlex.join([str(source), str(dest)]), check=False
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(dest.read_bytes(), b"old")
            dest.unlink()
            self.shell("copy_payload " + shlex.join([str(source), str(dest)]))
            self.assertEqual(dest.read_bytes(), b"new")
            self.shell("copy_payload " + shlex.join([str(source), str(dest)]))

    def test_low_space_refuses(self):
        with tempfile.TemporaryDirectory() as temp:
            result = self.shell(
                "check_space " + shlex.quote(temp) + " 999999999999999999", check=False
            )
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()

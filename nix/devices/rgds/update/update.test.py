#!/usr/bin/env python3
"""Offline public contracts: production renderer, real files and Nix profiles."""

import base64
import hashlib
import importlib.util
import io
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tarfile
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("rgds_update", HERE / "run.py")
update = importlib.util.module_from_spec(spec)
spec.loader.exec_module(update)
OLD = "/nix/store/" + "a" * 32 + "-nixos-system-rgds-sd-card-old"
NEW = "/nix/store/" + "b" * 32 + "-nixos-system-rgds-sd-card-new"
KERNEL = "c" * 32 + "-Image"
INITRD = "d" * 32 + "-initrd"
DTBS = "f" * 32 + "-dtbs"


def menu(system=OLD):
    return f"""# Native extlinux dialect
DEFAULT nixos-default
TIMEOUT 30
MENU TITLE ------------------------------------------------------------

LABEL nixos-default
  MENU LABEL NixOS - Default
  LINUX ../nixos/{KERNEL}
  INITRD ../nixos/{INITRD}
  APPEND init={system}/init quiet
  FDT ../nixos/{DTBS}/{update.DTB}
"""


class ExtlinuxTests(unittest.TestCase):
    def test_accepts_native_dialect_and_refuses_unsafe_or_unknown_directives(self):
        self.assertEqual(update.parse_extlinux(menu())[1][0]["system"], OLD)
        for text in (
            menu().replace("../nixos/", "../../nixos/"),
            menu().replace("  INITRD", "  UNKNOWN"),
            menu().replace(" quiet", " init=/bad/init quiet"),
            menu().replace(update.DTB, "other/board.dtb"),
            menu().replace("DEFAULT nixos-default", "DEFAULT other"),
            menu() + "  LINUX ../nixos/other\n",
            menu().replace("rgds-sd-card", "rpminiv2-sd-card"),
            menu() + "\n#" + "x" * 65536,
        ):
            with self.subTest(text=text), self.assertRaises(ValueError):
                update.parse_extlinux(text)

    @unittest.skipUnless(os.environ.get("RGDS_TEST_FIRMWARE"), "run rgds-update-check")
    def test_real_generator_archive_staging_and_exact_rollback(self):
        fixtures = Path(os.environ["RGDS_TEST_FIRMWARE"])
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            tree = root / "rendered"
            old, new = (
                (fixtures / "old-system").resolve(),
                (fixtures / "new-system").resolve(),
            )
            update.render(
                fixtures / "renderer", new, old, tree, timeout=3, dtb=update.DTB
            )
            expected = fixtures / "new"

            # Independent Nix fixture runs the real generator, including its old
            # generation entry/timestamp. Assert every byte and every pathname.
            def files(base):
                return {
                    p.relative_to(base).as_posix(): p.read_bytes()
                    for p in base.rglob("*")
                    if p.is_file()
                }

            self.assertEqual(files(tree), files(expected))
            self.assertEqual(
                [
                    e["system"]
                    for e in update.parse_extlinux(
                        (tree / "extlinux/extlinux.conf").read_text()
                    )[1]
                ],
                [str(new), str(old)],
            )
            archive, required = update.archive_payload(tree)
            stage = root / "stage"
            stage.mkdir()
            with tarfile.open(fileobj=io.BytesIO(archive)) as payload:
                self.assertTrue(all(m.isfile() for m in payload.getmembers()))
                payload.extractall(stage, filter="data")
            self.assertGreater(
                required, sum(p.stat().st_size for p in stage.rglob("*") if p.is_file())
            )
            boot = root / "boot"
            shutil.copytree(fixtures / "old", boot)
            for directory in [boot, *[p for p in boot.rglob("*") if p.is_dir()]]:
                directory.chmod(0o755)
            original = (boot / "extlinux/extlinux.conf").read_bytes()
            profile = root / "profiles/system"
            profile.parent.mkdir(exist_ok=True)
            TargetTests().shell(f"""
                cd {shlex.quote(str(stage))}
                sha256sum --check SHA256SUMS
                nix-env -p {shlex.quote(str(profile))} --set {shlex.quote(str(old))}
                install_payloads {shlex.quote(str(stage))} {shlex.quote(str(boot))}
                commit_update {shlex.quote(str(boot / "extlinux/extlinux.conf"))} {shlex.quote(str(profile))} {shlex.quote(str(stage / "extlinux.conf"))} {shlex.quote(str(new))}
                prune_payloads {shlex.quote(str(boot))}
            """)
            staged = {
                k: v for k, v in files(boot).items() if not k.endswith(".previous")
            }
            self.assertEqual(staged, files(expected))
            self.assertEqual(profile.resolve(), new)
            self.assertEqual(Path(str(profile) + "-rgds-previous").resolve(), old)
            TargetTests().shell(
                "rollback "
                + shlex.join([str(boot / "extlinux/extlinux.conf"), str(profile)])
            )
            self.assertEqual(profile.resolve(), old)
            self.assertEqual((boot / "extlinux/extlinux.conf").read_bytes(), original)


class TargetTests(unittest.TestCase):
    def shell(self, script, check=True):
        result = subprocess.run(
            [
                "bash",
                "-c",
                "set -euo pipefail; source "
                + shlex.quote(str(HERE / "target.sh"))
                + ";\n"
                + script,
            ],
            text=True,
            capture_output=True,
        )
        if check:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def test_mount_identity_refusals(self):
        args = ["/", "/dev/mmcblk0p2", "ext4", "NIXOS_RGDS", "rw,relatime"]
        self.shell("check_mount_values " + shlex.join(args))
        for i, value in [
            (0, "/boot"),
            (1, "/dev/mmcblk1p2"),
            (1, "/dev/mmcblk0p1"),
            (1, "/dev/mmcblk0p2[/bind]"),
            (2, "vfat"),
            (3, "NIXOS_BOOT"),
            (4, "ro,relatime"),
        ]:
            bad = args.copy()
            bad[i] = value
            with self.subTest(bad=bad):
                self.assertNotEqual(
                    self.shell(
                        "check_mount_values " + shlex.join(bad), False
                    ).returncode,
                    0,
                )

    def test_mbr_boot_selection_and_emmc_refusals(self):
        table = "label: dos\n/dev/mmcblk0p1 : start=32768, size=61440, type=b\n/dev/mmcblk0p2 : start=94208, size=99999, type=83, bootable\n"
        self.shell("check_layout " + shlex.join(["MMC", table]))
        for media, bad in [
            ("SD", table),
            ("MMC", table.replace("label: dos", "label: gpt")),
            ("MMC", table.replace(", bootable", "")),
            ("MMC", table.replace("type=b", "type=b, bootable")),
            ("MMC", table.replace("mmcblk0p2", "mmcblk1p2")),
        ]:
            with self.subTest(media=media, table=bad):
                self.assertNotEqual(
                    self.shell(
                        "check_layout " + shlex.join([media, bad]), False
                    ).returncode,
                    0,
                )

    def test_build_and_signature_policy_refusals(self):
        policy = "require-sigs = true\nmax-jobs = 0\nbuilders = \nfallback = false\n"
        self.shell("check_policy " + shlex.quote(policy))
        for old, new in [
            ("require-sigs = true", "require-sigs = false"),
            ("max-jobs = 0", "max-jobs = 1"),
            ("builders = ", "builders = ssh://builder"),
            ("fallback = false", "fallback = true"),
        ]:
            self.assertNotEqual(
                self.shell(
                    "check_policy " + shlex.quote(policy.replace(old, new)), False
                ).returncode,
                0,
            )

    def test_profile_failure_restores_exact_boot_selection(self):
        old = str(Path(shutil.which("bash")).resolve().parents[1])
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            cfg, candidate, profile = (
                root / "extlinux.conf",
                root / "candidate",
                root / "system",
            )
            original = menu(old)
            cfg.write_text(original)
            candidate.write_text(menu(NEW))
            self.shell(
                "nix-env -p " + shlex.quote(str(profile)) + " --set " + shlex.quote(old)
            )
            result = self.shell(
                "commit_update "
                + shlex.join([str(cfg), str(profile), str(candidate), NEW]),
                False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("restored previous selection", result.stderr)
            self.assertEqual(cfg.read_text(), original)
            self.assertEqual(str(profile.resolve()), old)

    def test_existing_payload_and_symlinks_are_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source, dest = root / "source", root / "dest"
            source.write_bytes(b"new")
            dest.write_bytes(b"old")
            command = "copy_payload " + shlex.join([str(source), str(dest)])
            self.assertNotEqual(self.shell(command, False).returncode, 0)
            self.assertEqual(dest.read_bytes(), b"old")
            dest.unlink()
            dest.symlink_to(source)
            self.assertNotEqual(self.shell(command, False).returncode, 0)
            dest.unlink()
            self.shell(command)
            self.shell(command)
            self.assertEqual(dest.read_bytes(), b"new")

    def test_low_space_refuses(self):
        with tempfile.TemporaryDirectory() as temp:
            self.assertNotEqual(
                self.shell(
                    "check_space " + shlex.quote(temp) + " 999999999999999999", False
                ).returncode,
                0,
            )

    def test_retention_keeps_active_and_backup_but_removes_stale_payload(self):
        with tempfile.TemporaryDirectory() as temp:
            boot = Path(temp)
            (boot / "extlinux").mkdir()
            (boot / "nixos").mkdir()
            (boot / "extlinux/extlinux.conf").write_text(menu(NEW))
            previous = menu(OLD).replace(KERNEL, "9" * 32 + "-old-Image")
            (boot / "extlinux/extlinux.conf.previous").write_text(previous)
            stale = boot / "nixos" / ("8" * 32 + "-stale")
            names = [KERNEL, INITRD, DTBS, "9" * 32 + "-old-Image", stale.name]
            for name in names:
                (boot / "nixos" / name).write_text("payload")
            self.shell("prune_payloads " + shlex.quote(str(boot)))
            self.assertFalse(stale.exists())
            self.assertEqual(len(list((boot / "nixos").iterdir())), 4)

    def test_rollback_refuses_backup_profile_disagreement(self):
        with tempfile.TemporaryDirectory() as temp:
            cfg, profile = Path(temp) / "extlinux.conf", Path(temp) / "system"
            cfg.write_text(menu(NEW))
            Path(str(cfg) + ".previous").write_text(menu(OLD))
            Path(str(profile) + "-rgds-previous").symlink_to(
                Path(shutil.which("bash")).resolve().parents[1]
            )
            result = self.shell(
                "rollback " + shlex.join([str(cfg), str(profile)]), False
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(cfg.read_text(), menu(NEW))


class CliTests(unittest.TestCase):
    def test_writes_need_expected_system_before_any_transport(self):
        for args in ([], ["--rollback"], ["--expected-system", "/bad"]):
            result = subprocess.run(
                ["python3", str(HERE / "run.py"), *args], capture_output=True, text=True
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("writes require --expected-system", result.stderr)

    def test_status_and_expected_system_with_offline_ssh_transport(self):
        # A configured subprocess transport, not a device and not a patched API.
        text = menu()
        status = f"current={OLD}\nprofile={OLD}\nextlinux_sha256={hashlib.sha256(text.encode()).hexdigest()}\nextlinux_base64={base64.b64encode(text.encode()).decode()}\n"
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            output = root / "status"
            output.write_text(status)
            transport = root / "ssh"
            transport.write_text('#!/usr/bin/env bash\ncat "$STATUS_OUTPUT"\n')
            transport.chmod(0o700)
            env = {
                **os.environ,
                "PATH": temp + os.pathsep + os.environ["PATH"],
                "STATUS_OUTPUT": str(output),
            }
            command = ["python3", str(HERE / "run.py")]
            result = subprocess.run(
                [*command, "--status"],
                env=env,
                capture_output=True,
                text=True,
                check=True,
            )
            self.assertIn("current=" + OLD, result.stdout)
            self.assertNotIn("base64", result.stdout)
            result = subprocess.run(
                [*command, "--rollback", "--expected-system", NEW],
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("does not match", result.stderr)
            result = subprocess.run(
                [*command, "--rollback", "--expected-system", OLD],
                env=env,
                capture_output=True,
                text=True,
                check=True,
            )
            self.assertIn("profile=" + OLD, result.stdout)
            output.write_text(
                status.replace(hashlib.sha256(text.encode()).hexdigest(), "0" * 64)
            )
            result = subprocess.run(
                [*command, "--status"], env=env, capture_output=True, text=True
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("checksum mismatch", result.stderr)


if __name__ == "__main__":
    unittest.main()

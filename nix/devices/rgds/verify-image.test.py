#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 util-linux e2fsprogs dosfstools
"""Offline acceptance tests with real filesystems, not executable boot payloads."""

from pathlib import Path
import os
import runpy
import subprocess
import sys
import tempfile
import unittest


VERIFY = Path(__file__).with_name("verify-image.py")
MIB = 1024 * 1024
FAT_START = 16 * MIB
FAT_SIZE = 30 * MIB
ROOT_START = FAT_START + FAT_SIZE
ROOT_SIZE = 32 * MIB
CONFIG = """DEFAULT nixos-default
TIMEOUT 30
LABEL nixos-default
  MENU LABEL NixOS - Default
  LINUX ../nixos/kernel
  INITRD ../nixos/initrd
  FDT ../nixos/dtbs/rockchip/rk3568-anbernic-rg-ds.dtb
"""


def run(*args, input=None):
    return subprocess.run(
        args, input=input, text=True, capture_output=True, check=True, timeout=30
    )


def copy_sparse(source, destination, offset):
    with source.open("rb") as src, destination.open("r+b") as dst:
        dst.seek(offset)
        while data := src.read(MIB):
            if data.count(0) == len(data):
                dst.seek(len(data), os.SEEK_CUR)
            else:
                dst.write(data)


class ImageAcceptance(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rgds-image-test-")
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.image = self.directory / "sd.img"
        self.uboot = self.directory / "u-boot-rockchip.bin"
        self.root = self.directory / "root.ext4"
        self.fat = self.directory / "firmware.fat"
        # These bytes only test storage and references. They cannot boot.
        self.uboot.write_bytes(b"uboot storage fixture\x00" * 64)
        tree = self.directory / "files"
        boot = tree / "boot"
        (boot / "extlinux").mkdir(parents=True)
        (boot / "nixos/dtbs/rockchip").mkdir(parents=True)
        (boot / "extlinux/extlinux.conf").write_text(CONFIG)
        for relative in ("kernel", "initrd", "dtbs/rockchip/rk3568-anbernic-rg-ds.dtb"):
            (boot / "nixos" / relative).write_bytes(
                b"storage fixture: " + relative.encode()
            )
        with self.root.open("wb") as root:
            root.truncate(ROOT_SIZE)
        run(
            "mkfs.ext4", "-q", "-F", "-L", "NIXOS_RGDS", "-d", str(tree), str(self.root)
        )
        with self.fat.open("wb") as fat:
            fat.truncate(FAT_SIZE)
        # Match pinned NixOS: mkfs.vfat chooses the FAT width for this size.
        run("mkfs.vfat", "--invariant", "-n", "NIXOS_BOOT", str(self.fat))
        with self.image.open("wb") as image:
            image.truncate(ROOT_START + ROOT_SIZE)
        run(
            "sfdisk",
            "--no-reread",
            "--no-tell-kernel",
            str(self.image),
            input=(
                "label: dos\n"
                f"start={FAT_START // 512}, size={FAT_SIZE // 512}, type=b\n"
                f"start={ROOT_START // 512}, size={ROOT_SIZE // 512}, type=83, bootable\n"
            ),
        )
        copy_sparse(self.fat, self.image, FAT_START)
        copy_sparse(self.root, self.image, ROOT_START)
        copy_sparse(self.uboot, self.image, 64 * 512)

    def verify(self, image=None, uboot=None):
        return subprocess.run(
            [
                sys.executable,
                str(VERIFY),
                str(image or self.image),
                str(uboot or self.uboot),
            ],
            capture_output=True,
            text=True,
            timeout=30,
        )

    def reject(self, message, **kwargs):
        result = self.verify(**kwargs)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(message, result.stderr)

    def update_root(self, command):
        run("debugfs", "-w", "-R", command, str(self.root))
        # Copy all bytes so deleted ext4 metadata replaces the original bytes.
        with self.root.open("rb") as src, self.image.open("r+b") as dst:
            dst.seek(ROOT_START)
            while data := src.read(MIB):
                dst.write(data)

    def test_valid_image_is_accepted_without_modifying_inputs(self):
        for path in (self.image, self.uboot):
            path.chmod(0o444)
        before = [
            (p.stat().st_size, p.stat().st_mtime_ns) for p in (self.image, self.uboot)
        ]
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("verified", result.stdout)
        self.assertEqual(
            before,
            [
                (p.stat().st_size, p.stat().st_mtime_ns)
                for p in (self.image, self.uboot)
            ],
        )

    def test_bad_embedded_uboot_is_rejected(self):
        with self.image.open("r+b") as image:
            image.seek(64 * 512)
            image.write(b"wrong")
        self.reject("U-Boot")

    def test_multichunk_uboot_and_second_chunk_corruption(self):
        self.uboot.write_bytes(b"x" * MIB + b"second chunk storage fixture")
        copy_sparse(self.uboot, self.image, 64 * 512)
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)
        with self.image.open("r+b") as image:
            image.seek(64 * 512 + MIB)
            image.write(b"wrong")
        self.reject("U-Boot")

    def test_valid_middle_default_ignores_broken_alternatives(self):
        config = self.directory / "extlinux.conf"
        config.write_text(
            CONFIG.replace(
                "LABEL nixos-default",
                "LABEL first\n  LINUX ../missing\nLABEL nixos-default",
            )
            + "LABEL last\n  INITRD ../missing\n"
        )
        self.update_root("rm /boot/extlinux/extlinux.conf")
        self.update_root(f"write {config} /boot/extlinux/extlinux.conf")
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_broken_default_does_not_use_valid_alternative(self):
        config = self.directory / "extlinux.conf"
        config.write_text(
            CONFIG.replace("LINUX ../nixos/kernel", "LINUX ../missing")
            + CONFIG.replace("DEFAULT nixos-default\n", "").replace(
                "LABEL nixos-default", "LABEL alternative"
            )
        )
        self.update_root("rm /boot/extlinux/extlinux.conf")
        self.update_root(f"write {config} /boot/extlinux/extlinux.conf")
        self.reject("/boot/missing")

    def test_wrong_root_label_is_rejected(self):
        self.update_root("set_super_value volume_name WRONG")
        self.reject("NIXOS_RGDS")

    def test_wrong_fat_label_is_rejected(self):
        run("mkfs.vfat", "--invariant", "-n", "WRONG", str(self.fat))
        copy_sparse(self.fat, self.image, FAT_START)
        self.reject("NIXOS_BOOT")

    def test_missing_boot_reference_is_rejected(self):
        self.update_root("rm /boot/nixos/initrd")
        self.reject("/boot/nixos/initrd")

    def test_empty_boot_reference_is_rejected(self):
        self.update_root("set_inode_field /boot/nixos/kernel size 0")
        self.reject("/boot/nixos/kernel")

    def test_missing_default_entry_is_rejected(self):
        config = self.directory / "extlinux.conf"
        config.write_text(CONFIG.replace("DEFAULT nixos-default", "DEFAULT missing"))
        self.update_root("rm /boot/extlinux/extlinux.conf")
        self.update_root(f"write {config} /boot/extlinux/extlinux.conf")
        self.reject("DEFAULT")

    def test_wrong_fdt_reference_is_rejected(self):
        config = self.directory / "extlinux.conf"
        config.write_text(
            CONFIG.replace("dtbs/rockchip/rk3568-anbernic-rg-ds.dtb", "kernel")
        )
        self.update_root("rm /boot/extlinux/extlinux.conf")
        self.update_root(f"write {config} /boot/extlinux/extlinux.conf")
        self.reject("rk3568-anbernic-rg-ds.dtb")

    def test_nonbootable_root_is_rejected(self):
        with self.image.open("r+b") as image:
            image.seek(446 + 16)
            image.write(b"\x00")
        self.reject("bootable")

    def test_truncated_image_is_rejected(self):
        with self.image.open("r+b") as image:
            image.truncate(ROOT_START + ROOT_SIZE - 512)
        self.reject("bounds")

    def test_ext4_geometry_exceeding_partition_is_rejected(self):
        with self.image.open("r+b") as image:
            image.seek(446 + 16 + 12)
            image.write(((ROOT_SIZE - MIB) // 512).to_bytes(4, "little"))
            image.truncate(ROOT_START + ROOT_SIZE - MIB)
        self.reject("ext4 geometry")

    def test_oversized_extlinux_config_is_rejected(self):
        self.update_root("set_inode_field /boot/extlinux/extlinux.conf size 67108864")
        self.reject("65536-byte config limit")

    def test_symlink_inputs_are_rejected(self):
        for name in ("image", "uboot"):
            with self.subTest(name=name):
                link = self.directory / f"{name}-link"
                link.symlink_to(getattr(self, name))
                self.reject("regular file", **{name: link})

    def test_empty_boot_directive_is_rejected(self):
        config = self.directory / "extlinux.conf"
        for directive in ("LINUX", "INITRD", "FDT"):
            with self.subTest(directive=directive):
                config.write_text(
                    "\n".join(
                        f"  {directive}" if line.strip().startswith(directive) else line
                        for line in CONFIG.splitlines()
                    )
                )
                self.update_root("rm /boot/extlinux/extlinux.conf")
                self.update_root(f"write {config} /boot/extlinux/extlinux.conf")
                self.reject(f"nonempty {directive}")

    def test_nonregular_input_is_rejected_without_opening_it(self):
        fifo = self.directory / "fifo"
        os.mkfifo(fifo)
        self.reject("regular file", image=fifo)
        self.reject("regular file", uboot=self.directory)

    def test_empty_uboot_is_rejected(self):
        self.uboot.write_bytes(b"")
        self.reject("U-Boot")

    def test_oversized_uboot_is_rejected(self):
        with self.uboot.open("wb") as uboot:
            uboot.truncate(FAT_START)
        self.reject("U-Boot")


class CommandOutputLimits(unittest.TestCase):
    def test_stdout_stderr_and_combined_output_are_bounded(self):
        command = runpy.run_path(str(VERIFY))["command"]
        for streams in ((1, 1), (2, 2), (1, 2)):
            with self.subTest(streams=streams):
                source = (
                    "import os\n"
                    f"for fd in {streams!r}:\n"
                    "    os.write(fd, b'x' * 700000)\n"
                )
                with self.assertRaisesRegex(ValueError, "output limit"):
                    command(sys.executable, "-c", source)

    def test_both_pipes_are_drained_below_limit(self):
        command = runpy.run_path(str(VERIFY))["command"]
        output = command(
            sys.executable,
            "-c",
            "import os; os.write(2, b'e' * 200000); os.write(1, b'o' * 200000)",
        )
        self.assertEqual(output, "o" * 200000)


if __name__ == "__main__":
    unittest.main()

"""Exercise file-image contracts, never block devices or mounts."""

import os
from pathlib import Path
import shutil
import struct
import subprocess
import tempfile
import unittest
import zlib

import boot


def script_image(text):
    payload = struct.pack(">II", len(text), 0) + text
    header = struct.pack(
        ">7I4B32s",
        0x27051956,
        0,
        0,
        len(payload),
        0,
        0,
        zlib.crc32(payload),
        5,
        7,
        6,
        1,
        b"",
    )
    return header[:4] + struct.pack(">I", zlib.crc32(header)) + header[8:] + payload


class BootContractTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.system = self.root / "system"
        self.system.mkdir()
        (self.system / "kernel").write_bytes(b"kernel from system")
        (self.system / "initrd").write_bytes(b"initrd from system")
        (self.system / "init").write_text("#!/bin/sh\n")
        (self.system / "kernel-params").write_text("console=tty0 panic=10\n")
        dtb = self.system / "dtbs" / boot.BOARD_DTB
        dtb.parent.mkdir(parents=True)
        dtb.write_bytes(b"board dtb from system")
        self.loader = self.root / "loader"
        self.loader.mkdir()
        self.text = b'setenv kernel_addr_r "0x09000000"\nsysboot mmc 1:1 any 0x100000 /extlinux/extlinux.conf\n'
        (self.loader / "boot.scr").write_bytes(
            script_image(boot.patch_script(self.text))
        )
        self.dest = self.root / "boot"

    def populate(self):
        boot.populate(self.system, self.loader, self.dest)

    def test_missing_initrd_fails_before_writing(self):
        (self.system / "initrd").unlink()
        with self.assertRaisesRegex(ValueError, "initrd"):
            self.populate()
        self.assertFalse(self.dest.exists())

    def test_wrong_archive_hash_fails_before_extracting(self):
        archive = self.root / "wrong.img.gz"
        archive.write_bytes(b"not the pinned release")
        output = self.root / "extracted"
        with self.assertRaisesRegex(ValueError, "SHA256 mismatch"):
            boot.extract(archive, output)
        self.assertFalse(output.exists())

    def test_truncated_partition_table_fails(self):
        image = self.root / "short.img"
        image.write_bytes(b"not an MBR")
        with self.assertRaisesRegex(ValueError, "partition table"):
            boot.verify_image(self.system, self.loader, image)

    def test_wrong_fat_size_fails(self):
        image = self.root / "small-fat.img"
        image.write_bytes(b"too small")
        with self.assertRaisesRegex(ValueError, "128 MiB"):
            boot.verify_fat(self.system, self.loader, image)

    def test_empty_initrd_fails(self):
        (self.system / "initrd").write_bytes(b"")
        with self.assertRaisesRegex(ValueError, "initrd"):
            self.populate()

    def test_missing_board_dtb_fails(self):
        (self.system / "dtbs" / boot.BOARD_DTB).unlink()
        with self.assertRaisesRegex(ValueError, "dtb"):
            self.populate()

    def test_duplicate_init_parameter_fails(self):
        (self.system / "kernel-params").write_text("console=tty0 init=/wrong/init")
        with self.assertRaisesRegex(ValueError, "init="):
            self.populate()

    def test_multiline_parameter_injection_fails(self):
        (self.system / "kernel-params").write_text("console=tty0\nINITRD /wrong")
        with self.assertRaisesRegex(ValueError, "single line"):
            self.populate()

    def test_no_sysboot_or_duplicate_sysboot_fails(self):
        for text in (b"echo boot\n", self.text + self.text):
            with self.subTest(text=text):
                with self.assertRaisesRegex(ValueError, "one sysboot"):
                    boot.patch_script(text)

    def test_already_patched_script_fails(self):
        with self.assertRaisesRegex(ValueError, "ramdisk_addr_r"):
            boot.patch_script(boot.patch_script(self.text))

    def test_corrupt_legacy_script_fails(self):
        image = script_image(self.text)
        for invalid in (image[:30], image[:-1], image[:-1] + b"X"):
            with self.subTest(length=len(invalid)):
                with self.assertRaises(ValueError):
                    boot.script_text(invalid)

    def test_population_preserves_system_contract_and_ignores_overrides(self):
        os.environ["KORRI_KERNEL"] = "/does-not-exist"
        self.addCleanup(os.environ.pop, "KORRI_KERNEL")
        self.populate()
        boot.verify_directory(self.system, self.loader, self.dest)
        conf = (self.dest / "extlinux/extlinux.conf").read_text()
        self.assertIn(f"APPEND init={self.system}/init console=tty0 panic=10\n", conf)
        self.assertIn("  FDTDIR /\n", conf)
        self.assertEqual(len(list(self.dest.glob("*.dtb"))), 9)
        self.assertEqual(
            (self.dest / "Image").read_bytes(), (self.system / "kernel").read_bytes()
        )

    def test_stale_destination_fails(self):
        self.dest.mkdir()
        (self.dest / "stale").touch()
        with self.assertRaisesRegex(ValueError, "empty"):
            self.populate()

    def test_missing_initrd_directive_is_rejected(self):
        self.populate()
        conf = self.dest / "extlinux/extlinux.conf"
        conf.write_text(conf.read_text().replace("  INITRD /initrd\n", ""))
        with self.assertRaisesRegex(ValueError, "extlinux"):
            boot.verify_directory(self.system, self.loader, self.dest)

    def test_wrong_dtb_alias_is_rejected(self):
        self.populate()
        (self.dest / boot.DTB_NAMES[0]).write_bytes(b"wrong board")
        with self.assertRaisesRegex(ValueError, "differs"):
            boot.verify_directory(self.system, self.loader, self.dest)

    def test_missing_patched_ramdisk_address_is_rejected(self):
        (self.loader / "boot.scr").write_bytes(script_image(self.text))
        with self.assertRaisesRegex(ValueError, "ramdisk_addr_r"):
            self.populate()

    def test_oversized_boot_fails_before_writing(self):
        with (self.system / "initrd").open("wb") as f:
            f.truncate(128 * 1024 * 1024)
        with self.assertRaisesRegex(ValueError, "128 MiB"):
            self.populate()
        self.assertFalse(self.dest.exists())

    def test_kernel_ram_load_gap_boundary(self):
        limit = 0x0C000000 - 0x09000000
        for size in (limit - 1, limit):
            with self.subTest(size=size):
                with (self.system / "kernel").open("wb") as target:
                    target.truncate(size)
                boot.contract(self.system, self.loader)
        with (self.system / "kernel").open("wb") as target:
            target.truncate(limit + 1)
        with self.assertRaisesRegex(ValueError, "48 MiB.*initrd"):
            self.populate()
        self.assertFalse(self.dest.exists())

    def root_fixture(self):
        # Real on-disk system/modulesTree shape: kernel.nix links the system to
        # modulesTree; aggregateModules links package subtrees and runs depmod.
        # Bytes are deterministic: this verifies identity, not the module ABI.
        self.modules = self.root / ("m" * 32 + "-kernel-modules")
        self.version = self.modules / "lib/modules/6.12.63"
        self.version.mkdir(parents=True)
        self.module = self.version / "extra/rk915.ko"
        self.module.parent.mkdir()
        self.module.write_bytes(b"rk915 module bytes\0\xff")
        (self.version / "modules.dep").write_text("extra/rk915.ko:\n")
        (self.version / "modules.dep.bin").write_bytes(b"dependency index\0\xff")
        (self.version / "modules.softdep").touch()
        self.package = self.root / ("k" * 32 + "-kernel-package")
        self.package_version = self.package / "lib/modules/6.12.63"
        self.package_module = self.package_version / "kernel/drivers/input.ko"
        self.package_module.parent.mkdir(parents=True)
        self.package_module.write_bytes(b"kernel input module\0\xff")
        (self.package_version / "modules.builtin").write_text("kernel/builtin.ko\n")
        (self.version / "kernel").symlink_to(self.package_version / "kernel")
        (self.version / "modules.builtin").symlink_to(
            os.path.relpath(self.package_version / "modules.builtin", self.version)
        )
        (self.version / "extra/input.ko").symlink_to("../kernel/drivers/input.ko")
        (self.system / "kernel-modules").symlink_to(self.modules)
        self.root_tree = self.root / "root-tree"
        self.root_tree.mkdir()
        for source in (self.system, self.modules, self.package):
            target = self.in_root(source)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(source, target, symlinks=True)

    def in_root(self, source):
        return self.root_tree / source.relative_to("/")

    def root_image(self, label="NIXOS_R36TMAX"):
        image = self.root / "root.img"
        with image.open("wb") as target:
            target.truncate(32 * 1024 * 1024)
        subprocess.run(
            [
                "mkfs.ext4",
                "-q",
                "-F",
                "-L",
                label,
                "-d",
                str(self.root_tree),
                str(image),
            ],
            check=True,
            capture_output=True,
        )
        return image

    def test_real_ext4_system_and_module_link_roundtrip(self):
        self.root_fixture()
        boot.verify_root(self.system, self.root_image())
        self.in_root(self.system / "init").unlink()
        with self.assertRaisesRegex(ValueError, "init"):
            boot.verify_root(self.system, self.root_image())

    def test_wrong_or_missing_root_label_is_rejected(self):
        self.root_fixture()
        for label in (
            "WRONG",
            "",
            " NIXOS_R36TMAX",
            "NIXOS_R36TMAX ",
            "NIXOS_R36TMAX\n",
            "NIXOS_R36TMAX\r",
        ):
            with self.subTest(label=label):
                with self.assertRaisesRegex(ValueError, "NIXOS_R36TMAX"):
                    boot.verify_root(self.system, self.root_image(label))

    def test_missing_module_is_rejected(self):
        self.root_fixture()
        self.in_root(self.module).unlink()
        with self.assertRaisesRegex(ValueError, "module"):
            boot.verify_root(self.system, self.root_image())

    def test_corrupt_module_is_rejected(self):
        self.root_fixture()
        self.in_root(self.module).write_bytes(b"corrupt module")
        with self.assertRaisesRegex(ValueError, "module"):
            boot.verify_root(self.system, self.root_image())

    def test_corrupt_dependency_index_is_rejected(self):
        self.root_fixture()
        self.in_root(self.version / "modules.dep.bin").write_bytes(b"corrupt index")
        with self.assertRaisesRegex(ValueError, "modules.dep.bin"):
            boot.verify_root(self.system, self.root_image())

    def test_missing_dependency_index_is_rejected(self):
        self.root_fixture()
        self.in_root(self.version / "modules.dep").unlink()
        with self.assertRaisesRegex(ValueError, "module"):
            boot.verify_root(self.system, self.root_image())

    def test_module_version_must_be_a_directory(self):
        self.root_fixture()
        target = self.in_root(self.version)
        shutil.rmtree(target)
        target.write_bytes(b"not a directory")
        with self.assertRaisesRegex(ValueError, "directory"):
            boot.verify_root(self.system, self.root_image())

    def test_module_root_must_be_a_directory(self):
        self.root_fixture()
        target = self.in_root(self.version.parent)
        shutil.rmtree(target)
        target.write_bytes(b"not a directory")
        with self.assertRaisesRegex(ValueError, "directory"):
            boot.verify_root(self.system, self.root_image())

    def test_extra_module_version_is_rejected(self):
        self.root_fixture()
        self.in_root(self.version.parent / "6.12.64").mkdir()
        with self.assertRaisesRegex(ValueError, "module directory"):
            boot.verify_root(self.system, self.root_image())

    def test_replaced_module_parent_directory_is_rejected(self):
        self.root_fixture()
        original = self.in_root(self.modules / "lib")
        replacement = self.modules / "replacement-lib"
        original.rename(self.in_root(replacement))
        original.symlink_to(replacement)
        with self.assertRaisesRegex(ValueError, "directory"):
            boot.verify_root(self.system, self.root_image())

    def test_dangling_module_link_does_not_resolve_against_host(self):
        self.root_fixture()
        self.in_root(self.package_module).unlink()
        self.assertTrue(self.package_module.is_file())
        with self.assertRaisesRegex(ValueError, "module"):
            boot.verify_root(self.system, self.root_image())

    def test_dangling_module_directory_link_is_rejected(self):
        self.root_fixture()
        shutil.rmtree(self.in_root(self.package_version / "kernel"))
        with self.assertRaisesRegex(ValueError, "module"):
            boot.verify_root(self.system, self.root_image())

    def test_module_symlink_loop_is_rejected(self):
        self.root_fixture()
        link = self.in_root(self.version / "extra/input.ko")
        link.unlink()
        link.symlink_to("input.ko")
        with self.assertRaisesRegex(ValueError, "symlink"):
            boot.verify_root(self.system, self.root_image())

    def test_corrupt_linked_module_is_rejected(self):
        self.root_fixture()
        self.in_root(self.package_module).write_bytes(b"corrupt linked module")
        with self.assertRaisesRegex(ValueError, "module"):
            boot.verify_root(self.system, self.root_image())

    def test_changed_module_link_target_is_rejected_even_with_identical_bytes(self):
        self.root_fixture()
        link = self.in_root(self.version / "extra/input.ko")
        link.unlink()
        link.symlink_to(self.package_module)
        with self.assertRaisesRegex(ValueError, "symlink"):
            boot.verify_root(self.system, self.root_image())

    def test_replaced_module_closure_is_rejected_even_with_identical_bytes(self):
        self.root_fixture()
        replacement = self.root / "replacement-modules"
        shutil.copytree(
            self.in_root(self.modules), self.in_root(replacement), symlinks=True
        )
        link = self.in_root(self.system / "kernel-modules")
        link.unlink()
        link.symlink_to(replacement)
        with self.assertRaisesRegex(ValueError, "kernel-modules"):
            boot.verify_root(self.system, self.root_image())

    def test_extra_module_is_rejected(self):
        self.root_fixture()
        self.in_root(self.version / "extra/unexpected.ko").write_bytes(b"extra module")
        with self.assertRaisesRegex(ValueError, "module"):
            boot.verify_root(self.system, self.root_image())

    def test_non_image_input_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "regular file"):
            boot.verify_fat(self.system, self.loader, self.root)

    def fat_image(self, label="NIXOS_BOOT"):
        if not self.dest.exists():
            self.populate()
        image = self.root / "fat.img"
        with image.open("wb") as f:
            f.truncate(boot.FAT_BYTES)
        subprocess.run(
            ["mkfs.vfat", "-n", label, str(image)],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        subprocess.run(
            ["mcopy", "-i", str(image), "-s", *map(str, self.dest.iterdir()), "::/"],
            check=True,
        )
        return image

    def test_wrong_or_missing_fat_label_is_rejected(self):
        for label in ("WRONG", ""):
            with self.subTest(label=label):
                with self.assertRaisesRegex(ValueError, "NIXOS_BOOT"):
                    boot.verify_fat(self.system, self.loader, self.fat_image(label))

    def test_real_fat_image_roundtrip_and_corruption(self):
        image = self.fat_image()
        boot.verify_fat(self.system, self.loader, image)
        subprocess.run(["mdel", "-i", str(image), "::/initrd"], check=True)
        with self.assertRaises((ValueError, subprocess.CalledProcessError)):
            boot.verify_fat(self.system, self.loader, image)


if __name__ == "__main__":
    unittest.main()

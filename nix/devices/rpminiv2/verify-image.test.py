#!/usr/bin/env nix-shell
#! nix-shell -i python3 -p python3 util-linux e2fsprogs dosfstools mtools dtc gptfdisk
"""CLI acceptance using real GPT/FAT/ext4 and compiled DTBs; not boot proof."""

import hashlib
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest

VERIFY = Path(__file__).with_name("verify-image.py")
MIB = 1024 * 1024
FAT_START = 8 * MIB
FAT_SIZE = 64 * MIB
ROOT_START = FAT_START + FAT_SIZE
ROOT_SIZE = 32 * MIB
HASH = "0" * 32
INIT = f"/nix/store/{HASH}-nixos-system-rpminiv2-test/init"
KERNEL = f"/EFI/nixos/{HASH}-linux-Image"
INITRD = f"/EFI/nixos/{HASH}-initrd-initrd"
DTB = f"/EFI/nixos/{HASH}-linux-sm8250-retroidpocket-rpminiv2.dtb"
ENTRY_PATH = "/loader/entries/nixos-generation-1.conf"
GRUB_PATH = "/boot/grub/grub.cfg"
GRUB_FONT = "/boot/grub/dejavu-mono.pf2"
PAYLOAD = b"nonempty storage fixture"
ENTRY = f"""title NixOS
version Generation 1 test
linux {KERNEL}
initrd {INITRD}
options init={INIT} console=tty0
devicetree {DTB}
"""
GRUB = f"""insmod part_gpt
insmod part_msdos
set timeout=2
set default=0
set timeout_style=menu
set lang=en_US
loadfont {GRUB_FONT}
set rotation=270
set gfxmode=auto
insmod efi_gop
insmod gfxterm
terminal_output gfxterm
set menu_color_normal=cyan/blue
set menu_color_highlight=white/blue
menuentry 'NixOS Retroid Pocket Mini V2' {{
  search --set -f {KERNEL}
  linux {KERNEL} init={INIT} console=tty0
  initrd {INITRD}
  devicetree {DTB}
}}
"""


def run(*args, input=None):
    return subprocess.run(
        args, input=input, text=True, capture_output=True, check=True, timeout=30
    )


def copy_partition(source, destination, offset):
    with source.open("rb") as src, destination.open("r+b") as dst:
        dst.seek(offset)
        while data := src.read(MIB):
            dst.write(data)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


class ProfileContract(unittest.TestCase):
    def test_recovery_and_korri_pin_distinct_kernel_and_dtb_hashes(self):
        module = runpy.run_path(VERIFY)
        for name in ("KERNEL_HASHES", "DTB_HASHES"):
            with self.subTest(name=name):
                hashes = module[name]
                self.assertEqual(set(hashes), {"recovery", "korri"})
                self.assertNotEqual(hashes["recovery"], hashes["korri"])
                for value in hashes.values():
                    self.assertRegex(value, r"^[0-9a-f]{64}$")


class ImageAcceptance(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="rpminiv2-test-")
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.image = self.directory / "sd.img"
        self.fat = self.directory / "esp.fat"
        self.root = self.directory / "root.ext4"
        self.tree = self.directory / "files"
        init = self.tree / INIT.lstrip("/")
        init.parent.mkdir(parents=True)
        init.write_text("#!/bin/sh\nexec /nix/store/stage-2\n")
        init.chmod(0o755)
        self.make_root()
        with self.fat.open("wb") as stream:
            stream.truncate(FAT_SIZE)
        run("mkfs.vfat", "--invariant", "-F", "32", "-n", "RPMINIV2", str(self.fat))
        for path in (
            "/EFI",
            "/EFI/BOOT",
            "/EFI/nixos",
            "/boot",
            "/boot/grub",
            "/loader",
            "/loader/entries",
        ):
            run("mmd", "-i", str(self.fat), "::" + path)
        # Boot payload bytes only exercise storage, not executable validity.
        for path in ("/EFI/BOOT/BOOTAA64.EFI", KERNEL, INITRD, GRUB_FONT):
            self.put(path, PAYLOAD, assemble=False)
        self.put(
            "/loader/loader.conf",
            "timeout 3\ndefault nixos-generation-1.conf\nconsole-mode keep\n",
            assemble=False,
        )
        self.put(ENTRY_PATH, ENTRY, assemble=False)
        self.put(GRUB_PATH, GRUB, assemble=False)
        self.put_dtb(assemble=False)
        self.expected_dtb_hash = digest(self.directory / "board.dtb")
        with self.image.open("wb") as stream:
            stream.truncate(ROOT_START + ROOT_SIZE + MIB)
        run(
            "sgdisk",
            "--clear",
            f"--new=1:{FAT_START // 512}:{ROOT_START // 512 - 1}",
            "--typecode=1:ef00",
            f"--new=2:{ROOT_START // 512}:{(ROOT_START + ROOT_SIZE) // 512 - 1}",
            "--typecode=2:8305",
            str(self.image),
        )
        copy_partition(self.fat, self.image, FAT_START)
        copy_partition(self.root, self.image, ROOT_START)

    def make_root(self):
        with self.root.open("wb") as stream:
            stream.truncate(ROOT_SIZE)
        run(
            "mkfs.ext4",
            "-q",
            "-F",
            "-L",
            "NIXOS_RPMINIV2",
            "-d",
            str(self.tree),
            str(self.root),
        )

    def put(self, path, content, assemble=True):
        source = self.directory / "payload"
        source.write_bytes(content.encode() if isinstance(content, str) else content)
        run("mcopy", "-o", "-i", str(self.fat), str(source), "::" + path)
        if assemble:
            copy_partition(self.fat, self.image, FAT_START)

    def put_dtb(
        self,
        model="Retroid Pocket Mini V2",
        compatible='"retroidpocket,rpminiv2", "qcom,sm8250"',
        extra="",
        assemble=True,
    ):
        source = self.directory / "board.dts"
        target = self.directory / "board.dtb"
        source.write_text(
            f'/dts-v1/; / {{ model = "{model}"; compatible = {compatible}; {extra} }};\n'
        )
        run("dtc", "-I", "dts", "-O", "dtb", "-o", str(target), str(source))
        self.put(DTB, target.read_bytes(), assemble=assemble)

    def verify(self, path=None, profile="recovery", use_profile_kernel=False):
        payload_hash = hashlib.sha256(PAYLOAD).hexdigest()
        command = [
            sys.executable,
            str(VERIFY),
            "--expected-loader-sha256",
            payload_hash,
            "--expected-dtb-sha256",
            self.expected_dtb_hash,
            "--expected-font-sha256",
            payload_hash,
        ]
        if not use_profile_kernel:
            command.extend(["--expected-kernel-sha256", payload_hash])
        command.extend(["--profile", profile, str(path or self.image)])
        return subprocess.run(
            command,
            text=True,
            capture_output=True,
            timeout=30,
        )

    def reject(self, path=None):
        result = self.verify(path)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("image rejected:", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_korri_profile_accepts_product_menu_title(self):
        product_grub = GRUB.replace(
            "menuentry 'NixOS Retroid Pocket Mini V2' {",
            "menuentry 'NixOS Retroid Pocket Mini V2 Korri' {",
        )
        self.put(GRUB_PATH, product_grub)
        result = self.verify(profile="korri")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("image verified:", result.stdout)

    def test_korri_profile_selects_pinned_product_kernel(self):
        product_kernel = Path(os.environ["RP_MINIV2_KORRI_KERNEL"])
        self.put(KERNEL, product_kernel.read_bytes())
        self.put(
            GRUB_PATH,
            GRUB.replace(
                "menuentry 'NixOS Retroid Pocket Mini V2' {",
                "menuentry 'NixOS Retroid Pocket Mini V2 Korri' {",
            ),
        )
        result = self.verify(profile="korri", use_profile_kernel=True)
        self.assertEqual(result.returncode, 0, result.stderr)

        self.put(GRUB_PATH, GRUB)
        result = self.verify(profile="recovery", use_profile_kernel=True)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(
            "kernel differs from the selected RP Mini V2 profile", result.stderr
        )

    def test_valid_image_is_read_only(self):
        self.image.chmod(0o444)
        before = (digest(self.image), self.image.stat().st_mtime_ns)
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("image verified:", result.stdout)
        self.assertEqual(before, (digest(self.image), self.image.stat().st_mtime_ns))

    def test_nixos_mbr_to_gpt_conversion(self):
        # Exercise the producer's conversion, not only a newly created GPT.
        run("sgdisk", "--zap-all", str(self.image))
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
        run("sgdisk", "--mbrtogpt", str(self.image))
        run("sgdisk", "--typecode=1:ef00", "--typecode=2:8305", str(self.image))
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_init_symlink_resolves_inside_root(self):
        init = self.tree / INIT.lstrip("/")
        init.unlink()
        target = init.parent / "stage-2"
        target.write_text("#!/bin/sh\nexit 0\n")
        init.symlink_to("stage-2")
        self.make_root()
        copy_partition(self.root, self.image, ROOT_START)
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_long_absolute_init_symlink(self):
        init = self.tree / INIT.lstrip("/")
        init.unlink()
        target = f"/nix/store/{'1' * 32}-stage-2-long-link-target-for-nixos/init"
        dest = self.tree / target.lstrip("/")
        dest.parent.mkdir()
        dest.write_text("#!/bin/sh\nexit 0\n")
        init.symlink_to(target)
        self.make_root()
        copy_partition(self.root, self.image, ROOT_START)
        result = self.verify()
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_empty_boot_files(self):
        for path in ("/EFI/BOOT/BOOTAA64.EFI", KERNEL, INITRD, DTB, GRUB_FONT):
            with self.subTest(path=path):
                self.put(path, b"")
                self.reject()
                if path == DTB:
                    self.put_dtb()
                else:
                    self.put(path, PAYLOAD)
                result = self.verify()
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_boot_files(self):
        for path in ("/EFI/BOOT/BOOTAA64.EFI", KERNEL, INITRD, DTB, GRUB_FONT):
            with self.subTest(path=path):
                result = self.verify()
                self.assertEqual(result.returncode, 0, result.stderr)
                run("mdel", "-i", str(self.fat), "::" + path)
                copy_partition(self.fat, self.image, FAT_START)
                self.reject()
                if path == DTB:
                    self.put_dtb()
                else:
                    self.put(path, PAYLOAD)

    def test_nonempty_wrong_provenance_bytes(self):
        for path in ("/EFI/BOOT/BOOTAA64.EFI", KERNEL, GRUB_FONT):
            with self.subTest(path=path):
                self.put(path, PAYLOAD + b" changed")
                self.reject()
                self.put(path, PAYLOAD)
        self.put_dtb(extra="test-marker;")
        self.reject()
        self.put_dtb()

    def test_bad_entry_references(self):
        variants = [
            ENTRY.replace(f"linux {KERNEL}\n", ""),
            ENTRY + f"linux {KERNEL}\n",
            ENTRY + f"initrd {INITRD}\n",
            ENTRY + f"devicetree {DTB}\n",
            ENTRY.replace(DTB, "/EFI/nixos/missing.dtb"),
            ENTRY.replace(KERNEL, "/EFI/nixos/../BOOT/BOOTAA64.EFI"),
            ENTRY.replace(KERNEL, "/EFI/nixos/a;command"),
            ENTRY.replace(KERNEL, KERNEL + " extra"),
            ENTRY.replace(KERNEL, KERNEL + "*"),
            ENTRY.replace(INITRD, KERNEL),
            ENTRY.replace(KERNEL, "EFI/nixos/relative"),
            ENTRY.replace(f"init={INIT}", ""),
            ENTRY.replace(f"init={INIT}", f"init={INIT} init={INIT}"),
            ENTRY.replace(INIT, "/nix/store/../../etc/passwd"),
            ENTRY.replace(INIT, INIT + '"'),
            ENTRY + "efi /EFI/BOOT/BOOTAA64.EFI\n",
        ]
        for entry in variants:
            with self.subTest(entry=entry):
                self.put(ENTRY_PATH, entry)
                self.reject()

    def test_active_grub_requires_exact_gop_setup(self):
        directives = [
            "insmod part_gpt",
            "insmod part_msdos",
            "set timeout=2",
            "set default=0",
            "set timeout_style=menu",
            "set lang=en_US",
            f"loadfont {GRUB_FONT}",
            "set rotation=270",
            "set gfxmode=auto",
            "insmod efi_gop",
            "insmod gfxterm",
            "terminal_output gfxterm",
            "set menu_color_normal=cyan/blue",
            "set menu_color_highlight=white/blue",
        ]
        for directive in directives:
            with self.subTest(directive=directive):
                self.assertIn(directive + "\n", GRUB)
                self.put(GRUB_PATH, GRUB.replace(directive + "\n", "", 1))
                self.reject()

    def test_active_grub_must_match_metadata(self):
        variants = [
            GRUB.replace(f"  linux {KERNEL} init={INIT} console=tty0\n", ""),
            GRUB + f"menuentry 'Other' {{\n  linux {KERNEL} x\n}}\n",
            GRUB.replace(
                f"  initrd {INITRD}\n", f"  initrd {INITRD}\n  initrd {INITRD}\n"
            ),
            GRUB.replace(KERNEL, "/EFI/nixos/missing-Image", 1),
            GRUB.replace("console=tty0", "console=ttyMSM0"),
            GRUB.replace(
                "  devicetree", "  chainloader /EFI/BOOT/BOOTAA64.EFI\n  devicetree"
            ),
            GRUB.removesuffix("}\n"),
        ]
        for grub in variants:
            with self.subTest(grub=grub):
                self.put(GRUB_PATH, grub)
                self.reject()

    def test_missing_or_empty_grub_config(self):
        self.put(GRUB_PATH, b"")
        self.reject()
        self.put(GRUB_PATH, GRUB)
        run("mdel", "-i", str(self.fat), "::" + GRUB_PATH)
        copy_partition(self.fat, self.image, FAT_START)
        self.reject()

    def test_bad_loader_selection(self):
        for content in (
            "timeout 3\n",
            "default missing.conf\n",
            "default nixos-generation-*.conf\n",
            "default nixos-generation-1.conf\ndefault nixos-generation-1.conf\n",
            "default ../nixos-generation-1.conf\n",
        ):
            with self.subTest(content=content):
                self.put("/loader/loader.conf", content)
                self.reject()

    def test_missing_active_entry_does_not_use_another_entry(self):
        self.put("/loader/entries/nixos-generation-2.conf", ENTRY)
        run("mdel", "-i", str(self.fat), "::" + ENTRY_PATH)
        copy_partition(self.fat, self.image, FAT_START)
        self.reject()

    def test_wrong_selected_dtb_does_not_use_another_dtb(self):
        source = self.directory / "board.dtb"
        self.put(f"/EFI/nixos/{HASH}-correct-but-inactive.dtb", source.read_bytes())
        self.put_dtb(model="Retroid Pocket Mini")
        self.reject()

    def test_malformed_config(self):
        self.put(ENTRY_PATH, b"\xff\x00")
        self.reject()

    def test_oversized_config(self):
        self.put(ENTRY_PATH, ENTRY + "#" * 65536)
        self.reject()

    def test_subprocess_output_is_bounded(self):
        self.put(ENTRY_PATH, "#" * (2 * MIB))
        result = self.verify()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exceeded output limit", result.stderr)

    def test_wrong_dtb_identity(self):
        self.put_dtb(model="Retroid Pocket Mini")
        self.reject()
        self.put_dtb(compatible='"retroidpocket,rpmini", "qcom,sm8250"')
        self.reject()
        self.put_dtb(compatible='"retroidpocket,rpminiv2", "qcom,sm8550"')
        self.reject()

    def test_compatible_requires_two_nul_delimited_strings(self):
        self.put_dtb(compatible='"retroidpocket,rpminiv2 qcom,sm8250"')
        self.reject()

    def test_corrupt_dtb(self):
        self.put(DTB, b"not a flattened device tree")
        self.reject()

    def test_bad_init(self):
        init = self.tree / INIT.lstrip("/")
        for kind in ("missing", "empty", "dangling", "loop", "directory", "host-path"):
            with self.subTest(kind=kind):
                if init.is_symlink() or init.is_file():
                    init.unlink()
                elif init.is_dir():
                    init.rmdir()
                if kind == "empty":
                    init.touch()
                elif kind == "dangling":
                    init.symlink_to("missing")
                elif kind == "loop":
                    init.symlink_to("init")
                elif kind == "directory":
                    init.mkdir()
                elif kind == "host-path":
                    init.symlink_to("/etc/passwd")
                self.make_root()
                copy_partition(self.root, self.image, ROOT_START)
                self.reject()

    def test_wrong_fat_label(self):
        run("mlabel", "-i", str(self.fat), "::WRONG")
        copy_partition(self.fat, self.image, FAT_START)
        self.reject()

    def test_wrong_root_label(self):
        run("e2label", str(self.root), "WRONG")
        copy_partition(self.root, self.image, ROOT_START)
        self.reject()

    def test_wrong_partition_type(self):
        run("sgdisk", "--typecode=2:8300", str(self.image))
        self.reject()

    def test_extra_partition(self):
        run("sgdisk", "--new=3:2048:4095", str(self.image))
        self.reject()

    def test_wrong_first_partition_offset(self):
        run(
            "sgdisk",
            "--delete=1",
            f"--new=1:8192:{ROOT_START // 512 - 1}",
            "--typecode=1:ef00",
            str(self.image),
        )
        self.reject()

    def test_partition_gap(self):
        run(
            "sgdisk",
            "--delete=2",
            f"--new=2:{ROOT_START // 512 + 2048}:{(ROOT_START + ROOT_SIZE) // 512 - 1}",
            "--typecode=2:8305",
            str(self.image),
        )
        self.reject()

    def test_truncated_image(self):
        with self.image.open("r+b") as stream:
            stream.truncate(ROOT_START + MIB)
        self.reject()

    def test_corrupt_gpt_headers_and_tables(self):
        for offset in (
            510,
            512,
            536,
            1024,
            self.image.stat().st_size - 33 * 512,
            self.image.stat().st_size - 512,
        ):
            with self.subTest(offset=offset):
                with self.image.open("r+b") as stream:
                    stream.seek(offset)
                    original = stream.read(1)
                    stream.seek(offset)
                    stream.write(bytes([original[0] ^ 255]))
                self.reject()
                with self.image.open("r+b") as stream:
                    stream.seek(offset)
                    stream.write(original)

    def test_corrupt_filesystem_signatures_and_geometry(self):
        for offset, data in (
            (FAT_START + 510, b"xx"),
            (FAT_START + 32, (0xFFFFFFFF).to_bytes(4, "little")),
            (ROOT_START + 1024 + 56, b"xx"),
            (ROOT_START + 1024 + 4, (0xFFFFFFFF).to_bytes(4, "little")),
        ):
            with self.subTest(offset=offset):
                with self.image.open("r+b") as stream:
                    stream.seek(offset)
                    original = stream.read(len(data))
                    stream.seek(offset)
                    stream.write(data)
                self.reject()
                with self.image.open("r+b") as stream:
                    stream.seek(offset)
                    stream.write(original)

    def test_nonregular_inputs_rejected_without_blocking(self):
        link = self.directory / "link.img"
        link.symlink_to(self.image)
        fifo = self.directory / "fifo"
        os.mkfifo(fifo)
        for path in (
            link,
            fifo,
            self.directory,
            Path("/dev/null"),
            self.directory / "missing",
        ):
            with self.subTest(path=path):
                self.reject(path)


if __name__ == "__main__":
    unittest.main()

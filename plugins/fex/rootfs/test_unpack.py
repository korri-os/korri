"""Run the real fex-unpack source against small synthetic archives."""
import os
from pathlib import Path
import random
import subprocess
import tempfile
import unittest

HERE = Path(__file__).parent


def elf(machine):
    return b"\x7fELF" + bytes(14) + machine + bytes(64)


def write_tree(root, files):
    for name, content in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)


def tar_zst(source, files, extra_names=(".PKGINFO", ".MTREE")):
    write_tree(source, files | {name: b"metadata\n" for name in extra_names})
    archive = source.with_suffix(".pkg.tar.zst")
    subprocess.run(["tar", "--zstd", "-cf", str(archive), "-C", str(source), "."], check=True)
    return archive


def fixtures(scratch, icd_machine=b"\x3e\x00"):
    """The base image and three overlay packages, in declared order."""
    base = scratch / "base"
    write_tree(base, {
        "etc/os-release": b"NAME=Fixture\n",
        # The overlay replaces this wrong-architecture driver.
        "usr/lib/libvulkan_freedreno.so": elf(b"\xb7\x00"),
        "usr/share/locked/file": b"inside a read-only directory\n",
        "usr/share/blob": random.Random(0).randbytes(64 * 1024),
    })
    (base / "usr/share/locked").chmod(0o555)
    (base / "usr/lib/link").symlink_to("libvulkan_freedreno.so")
    image = scratch / "ArchLinux.sqsh"
    subprocess.run(["mksquashfs", str(base), str(image), "-noappend", "-quiet"], check=True,
                   stdout=subprocess.DEVNULL)
    (base / "usr/share/locked").chmod(0o755)
    gallium = "libgallium-26.1.2-arch1.1.so"
    return [
        image,
        tar_zst(scratch / "mesa", {
            f"usr/lib/{gallium}": b"x86_64 gallium\n",
            f"usr/lib32/{gallium}": b"i686 gallium\n",
        }),
        tar_zst(scratch / "freedreno", {
            "usr/share/vulkan/icd.d/freedreno_icd.x86_64.json": b"{}\n",
            "usr/share/vulkan/icd.d/freedreno_icd.i686.json": b"{}\n",
            "usr/lib/libvulkan_freedreno.so": elf(icd_machine),
            "usr/lib32/libvulkan_freedreno.so": elf(b"\x03\x00"),
        }),
        tar_zst(scratch / "libraries", {
            "usr/lib/libdisplay-info.so.3": b"x\n",
            "usr/lib32/libdisplay-info.so.3": b"x\n",
            "usr/lib/libxcb-keysyms.so.1": b"x\n",
            "usr/lib32/libxcb-keysyms.so.1": b"x\n",
        }),
    ]


class FexUnpackTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.addCleanup(lambda: subprocess.run(["chmod", "-R", "u+w", temporary.name], check=False))
        self.root = Path(temporary.name)
        # korrid creates the work directory inside the contribution directory.
        self.contribution = self.root / "cache/fex/rootfs"
        self.work = self.contribution / ".tmp-fixture"
        self.work.mkdir(parents=True)
        self.files = fixtures(self.root / "fixtures")

    def run_unpack(self, *arguments):
        return subprocess.run(["bash", str(HERE / "unpack.sh"), *map(str, arguments)],
                              capture_output=True, text=True, timeout=60)

    def test_unpacks_the_squashfs_and_overlays_in_order_into_the_work_directory(self):
        result = self.run_unpack(self.work, *self.files)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")
        self.assertEqual((self.work / "etc/os-release").read_text(), "NAME=Fixture\n")
        self.assertEqual((self.work / "usr/lib/libvulkan_freedreno.so").read_bytes()[18:20], b"\x3e\x00")
        self.assertEqual((self.work / "usr/lib/libgallium-26.1.2-arch1.1.so").read_text(), "x86_64 gallium\n")
        self.assertEqual((self.work / "usr/share/locked/file").read_text(), "inside a read-only directory\n")
        self.assertTrue((self.work / "usr/lib/link").is_symlink())
        for metadata in (".PKGINFO", ".MTREE"):
            self.assertFalse((self.work / metadata).exists())
        # korrid flips the tree read-only and renames it; the program leaves
        # it writable and writes nowhere else.
        self.assertTrue((self.work / "etc/os-release").stat().st_mode & 0o200)
        self.assertEqual(sorted(p.name for p in self.contribution.iterdir()), [".tmp-fixture"])

    def test_wrong_freedreno_machine_type_fails(self):
        self.files = fixtures(self.root / "wrong-icd", icd_machine=b"\xb7\x00")
        result = self.run_unpack(self.work, *self.files)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Freedreno ICD is not x86_64", result.stderr)

    def test_missing_overlay_file_fails(self):
        result = self.run_unpack(self.work, *self.files[:3])
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("rootfs is missing usr/lib/libdisplay-info.so.3", result.stderr)

    def test_usage_errors_exit_1_without_stdout(self):
        for arguments in (
            [],
            [self.work],
            ["relative", *self.files],
            [self.root / "missing", *self.files],
            [self.work, self.root / "missing.sqsh", *self.files[1:]],
        ):
            with self.subTest(arguments=arguments):
                result = self.run_unpack(*arguments)
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertEqual(os.listdir(self.work), [])


if __name__ == "__main__":
    unittest.main()

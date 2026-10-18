"""Run the real fex-prepare source against a local fixture server, never the internet."""
import hashlib
import random
from pathlib import Path
import subprocess
import tempfile
import unittest

from fixture_server import FixtureServer

HERE = Path(__file__).parent
VERSION = "archlinux-fixture"
PARTIAL = 64 * 1024


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
    return archive.read_bytes()


def fixtures(scratch, icd_machine=b"\x3e\x00"):
    """The base image and three overlay packages, in pin order."""
    base = scratch / "base"
    write_tree(base, {
        "etc/os-release": b"NAME=Fixture\n",
        # The overlay replaces this wrong-architecture driver.
        "usr/lib/libvulkan_freedreno.so": elf(b"\xb7\x00"),
        "usr/share/locked/file": b"inside a read-only directory\n",
        # Incompressible bytes make the image large enough to interrupt.
        "usr/share/blob": random.Random(0).randbytes(256 * 1024),
    })
    (base / "usr/share/locked").chmod(0o555)
    (base / "usr/lib/link").symlink_to("libvulkan_freedreno.so")
    image = scratch / "ArchLinux.sqsh"
    subprocess.run(["mksquashfs", str(base), str(image), "-noappend", "-quiet"], check=True,
                   stdout=subprocess.DEVNULL)
    (base / "usr/share/locked").chmod(0o755)
    gallium = "libgallium-26.1.2-arch1.1.so"
    return [
        ("base", "ArchLinux.sqsh", image.read_bytes()),
        ("overlay", "mesa.pkg.tar.zst", tar_zst(scratch / "mesa", {
            f"usr/lib/{gallium}": b"x86_64 gallium\n",
            f"usr/lib32/{gallium}": b"i686 gallium\n",
        })),
        ("overlay", "freedreno.pkg.tar.zst", tar_zst(scratch / "freedreno", {
            "usr/share/vulkan/icd.d/freedreno_icd.x86_64.json": b"{}\n",
            "usr/share/vulkan/icd.d/freedreno_icd.i686.json": b"{}\n",
            "usr/lib/libvulkan_freedreno.so": elf(icd_machine),
            "usr/lib32/libvulkan_freedreno.so": elf(b"\x03\x00"),
        })),
        ("overlay", "libraries.pkg.tar.zst", tar_zst(scratch / "libraries", {
            "usr/lib/libdisplay-info.so.3": b"x\n",
            "usr/lib32/libdisplay-info.so.3": b"x\n",
            "usr/lib/libxcb-keysyms.so.1": b"x\n",
            "usr/lib32/libxcb-keysyms.so.1": b"x\n",
        })),
    ]


class FexPrepareTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.addCleanup(lambda: subprocess.run(["chmod", "-R", "u+w", temporary.name], check=False))
        self.root = Path(temporary.name)
        self.cache = self.root / "cache"
        self.cache.mkdir()
        self.rootfs = self.cache / "fex/rootfs"
        self.ready = self.rootfs / VERSION
        self.pins = fixtures(self.root / "fixtures")

    def serve(self, **knobs):
        server = FixtureServer({name: data for _, name, data in self.pins}, **knobs)
        self.addCleanup(server.stop)
        return server

    def script(self, server, wrong=None):
        lines = []
        for kind, name, data in self.pins:
            digest = hashlib.sha256(data + (b"!" if name == wrong else b"")).hexdigest()
            lines.append(f"{kind} {digest} {server.url(name)}\n")
        pins = self.root / "pins"
        pins.write_text("".join(lines))
        source = (HERE / "prepare.sh").read_text()
        source = source.replace("@pins@", str(pins)).replace("@version@", VERSION)
        path = self.root / "fex-prepare"
        path.write_text(source)
        return path

    def run_prepare(self, script, *args):
        arguments = args if args else (str(self.cache),)
        return subprocess.run(["bash", str(script), *arguments], capture_output=True, text=True,
                              timeout=60)

    def assert_no_partial_result(self):
        self.assertFalse(self.ready.exists())
        self.assertEqual(list(self.rootfs.glob(".tmp-*")), [])

    def sha(self, name):
        return next(hashlib.sha256(data).hexdigest() for _, n, data in self.pins if n == name)

    def test_prepares_the_overlaid_rootfs_and_prints_it_last(self):
        server = self.serve()
        result = self.run_prepare(self.script(server))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines()[-1], str(self.ready))
        self.assertEqual((self.ready / "etc/os-release").read_text(), "NAME=Fixture\n")
        self.assertEqual((self.ready / "usr/lib/libvulkan_freedreno.so").read_bytes()[18:20], b"\x3e\x00")
        self.assertTrue((self.ready / "usr/lib/link").is_symlink())
        for metadata in (".PKGINFO", ".MTREE"):
            self.assertFalse((self.ready / metadata).exists())
        # The ready tree is never edited in place.
        self.assertEqual((self.ready / "etc/os-release").stat().st_mode & 0o222, 0)
        self.assertEqual(self.ready.stat().st_mode & 0o222, 0)
        self.assertFalse((self.rootfs / ".downloads").exists())
        self.assertEqual(list(self.rootfs.glob(".tmp-*")), [])

    def test_existing_ready_directory_is_a_no_op(self):
        server = self.serve()
        script = self.script(server)
        self.assertEqual(self.run_prepare(script).returncode, 0)
        requests = len(server.requests)
        result = self.run_prepare(script)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines()[-1], str(self.ready))
        self.assertEqual(len(server.requests), requests)

    def test_hash_mismatch_is_refused_and_leaves_nothing(self):
        for name in ("ArchLinux.sqsh", "freedreno.pkg.tar.zst"):
            with self.subTest(name=name):
                server = self.serve()
                result = self.run_prepare(self.script(server, wrong=name))
                self.assertEqual(result.returncode, 1, result.stderr)
                self.assertIn("hash mismatch", result.stderr)
                self.assert_no_partial_result()
                self.assertEqual(list((self.rootfs / ".downloads").glob("*.part")), [])

    def test_failed_verification_leaves_no_partial_directory(self):
        self.pins = fixtures(self.root / "wrong-icd", icd_machine=b"\xb7\x00")
        result = self.run_prepare(self.script(self.serve()))
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("Freedreno ICD is not x86_64", result.stderr)
        self.assert_no_partial_result()

    def test_interrupted_run_work_directory_is_removed(self):
        stale = self.rootfs / ".tmp-interrupted/usr"
        stale.mkdir(parents=True)
        stale.chmod(0o555)
        result = self.run_prepare(self.script(self.serve()))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(stale.parent.exists())

    def test_dropped_download_is_resumed_by_the_next_run(self):
        server = self.serve(truncate=PARTIAL)
        script = self.script(server)
        first = self.run_prepare(script)
        self.assertEqual(first.returncode, 1, first.stderr)
        self.assertIn("download failed", first.stderr)
        self.assert_no_partial_result()
        part = self.rootfs / ".downloads" / f"{self.sha('ArchLinux.sqsh')}.part"
        self.assertEqual(part.stat().st_size, PARTIAL)
        second = self.run_prepare(script)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertIn(("ArchLinux.sqsh", f"bytes={PARTIAL}-"), server.requests)
        self.assertTrue(self.ready.is_dir())

    def test_partial_download_restarts_when_the_server_cannot_resume(self):
        server = self.serve(ranges=False)
        self.write_part("ArchLinux.sqsh", lambda data: data[:PARTIAL])
        result = self.run_prepare(self.script(server))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("restarting", result.stderr)
        base = [request for request in server.requests if request[0] == "ArchLinux.sqsh"]
        self.assertEqual(base, [("ArchLinux.sqsh", f"bytes={PARTIAL}-"), ("ArchLinux.sqsh", None)])

    def test_corrupt_partial_download_restarts(self):
        server = self.serve()
        self.write_part("ArchLinux.sqsh", lambda data: b"\0" * PARTIAL)
        result = self.run_prepare(self.script(server))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("do not match; restarting", result.stderr)

    def test_concurrent_runs_download_once(self):
        server = self.serve(delay=0.2)
        script = self.script(server)
        runs = [subprocess.Popen(["bash", str(script), str(self.cache)], stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, text=True) for _ in range(2)]
        for run in runs:
            stdout, stderr = run.communicate(timeout=60)
            self.assertEqual(run.returncode, 0, stderr)
            self.assertEqual(stdout.splitlines()[-1], str(self.ready))
        base = [request for request in server.requests if request[0] == "ArchLinux.sqsh"]
        self.assertEqual(len(base), 1)

    def test_usage_errors_exit_2(self):
        script = self.script(self.serve())
        for arguments in ([], ["relative"], [str(self.root / "missing")], [str(self.cache), "extra"]):
            with self.subTest(arguments=arguments):
                result = subprocess.run(["bash", str(script), *arguments], capture_output=True,
                                        text=True, timeout=10)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("usage: fex-prepare <cache-root>", result.stderr)
                self.assertEqual(result.stdout, "")

    def write_part(self, name, content):
        data = next(data for _, n, data in self.pins if n == name)
        downloads = self.rootfs / ".downloads"
        downloads.mkdir(parents=True)
        (downloads / f"{self.sha(name)}.part").write_bytes(content(data))


if __name__ == "__main__":
    unittest.main()

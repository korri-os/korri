"""Run the real cachyos-prepare source against a local fixture server, never the internet."""
import fcntl
import hashlib
import os
from pathlib import Path
import random
import subprocess
import tempfile
import time
import unittest

from fixture_server import FixtureServer

HERE = Path(__file__).parent
VERSION = "fixture-arm64"
TOOL = "proton-cachyos-fixture-arm64"
ARCHIVE = "proton.tar.xz"
PARTIAL = 64 * 1024
UPSTREAM_VDF = '"compatibilitytools" { "compat_tools" { "upstream" { "install_path" "." } } }\n'


def release(scratch, files=None):
    """A tar.xz with one top-level dist directory, like the CachyOS release."""
    dist = scratch / "proton-cachyos-fixture"
    contents = files if files is not None else {
        "proton": "#!/usr/bin/env python3\n",
        "toolmanifest.vdf": '"manifest"\n{\n  "version" "2"\n  "require_tool_appid" "4185400"\n'
                            '  "commandline" "/proton %verb%"\n}\n',
        "compatibilitytool.vdf": UPSTREAM_VDF,
        "files/lib/wine.so": "native\n",
    }
    for name, content in contents.items():
        path = dist / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
    # Incompressible bytes make the archive large enough to interrupt.
    (dist / "files/blob").parent.mkdir(parents=True, exist_ok=True)
    (dist / "files/blob").write_bytes(random.Random(0).randbytes(256 * 1024))
    archive = scratch / ARCHIVE
    subprocess.run(["tar", "--xz", "-cf", str(archive), "-C", str(scratch), dist.name], check=True)
    return archive.read_bytes()


class CachyosPrepareTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.addCleanup(lambda: subprocess.run(["chmod", "-R", "u+w", temporary.name], check=False))
        self.root = Path(temporary.name)
        self.cache = self.root / "cache"
        self.cache.mkdir()
        self.tools = self.cache / "proton/cachyos"
        self.ready = self.tools / VERSION
        self.archive = release(self.root / "release")
        self.vdf = self.root / "compatibilitytool.vdf"
        self.vdf.write_text((HERE / "compatibilitytool.vdf").read_text().replace("@toolName@", TOOL))

    def serve(self, **knobs):
        server = FixtureServer({ARCHIVE: self.archive}, **knobs)
        self.addCleanup(server.stop)
        return server

    def script(self, server, sha=None):
        source = (HERE / "prepare.sh").read_text()
        for placeholder, value in {
            "url": server.url(ARCHIVE),
            "sha256": sha or hashlib.sha256(self.archive).hexdigest(),
            "version": VERSION,
            "compatibilitytool": str(self.vdf),
        }.items():
            source = source.replace(f"@{placeholder}@", value)
        path = self.root / "cachyos-prepare"
        path.write_text(source)
        return path

    def run_prepare(self, script, *arguments):
        return subprocess.run(["bash", str(script), *(arguments or (str(self.cache),))],
                              capture_output=True, text=True, timeout=60)

    def assert_no_partial_result(self):
        self.assertFalse(self.ready.exists())
        self.assertEqual(list(self.tools.glob(".tmp-*")), [])

    def part(self):
        return self.tools / ".downloads" / f"{hashlib.sha256(self.archive).hexdigest()}.part"

    def test_prepares_the_tool_directory_and_prints_it_last(self):
        result = self.run_prepare(self.script(self.serve()))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines()[-1], str(self.ready))
        self.assertTrue((self.ready / "proton").is_file())
        self.assertEqual((self.ready / "files/lib/wine.so").read_text(), "native\n")
        manifest = (self.ready / "toolmanifest.vdf").read_text()
        self.assertNotIn("require_tool_appid", manifest)
        self.assertIn('"commandline" "/proton %verb%"', manifest)
        # Korri's registration replaces the release's own file.
        vdf = (self.ready / "compatibilitytool.vdf").read_text()
        self.assertIn(f'"{TOOL}"', vdf)
        self.assertIn('"install_path" "."', vdf)
        self.assertEqual((self.ready / "proton").stat().st_mode & 0o222, 0)
        self.assertFalse((self.tools / ".downloads").exists())
        self.assertEqual(list(self.tools.glob(".tmp-*")), [])

    def test_existing_ready_directory_is_a_no_op(self):
        server = self.serve()
        script = self.script(server)
        self.assertEqual(self.run_prepare(script).returncode, 0)
        result = self.run_prepare(script)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines()[-1], str(self.ready))
        self.assertEqual(len(server.requests), 1)

    def test_ready_directory_with_leftovers_is_cleaned_under_the_lock(self):
        server = self.serve()
        script = self.script(server)
        self.assertEqual(self.run_prepare(script).returncode, 0)
        requests = len(server.requests)
        # A run killed after its rename leaves downloads and work behind.
        downloads = self.tools / ".downloads"
        downloads.mkdir()
        (downloads / ("0" * 64 + ".part")).write_bytes(b"partial")
        stale = self.tools / ".tmp-killed/locked"
        stale.mkdir(parents=True)
        stale.chmod(0o555)
        with open(self.tools / ".lock", "w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            run = subprocess.Popen(["bash", str(script), str(self.cache)], stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True)
            time.sleep(0.5)
            self.assertIsNone(run.poll(), "cleanup must wait for the lock")
            self.assertTrue(downloads.exists())
        stdout, stderr = run.communicate(timeout=30)
        self.assertEqual(run.returncode, 0, stderr)
        self.assertEqual(stdout.splitlines()[-1], str(self.ready))
        self.assertFalse(downloads.exists())
        self.assertEqual(list(self.tools.glob(".tmp-*")), [])
        self.assertTrue(self.ready.is_dir())
        self.assertEqual(len(server.requests), requests)

    @unittest.skipIf(os.geteuid() == 0, "root can remove entries from a read-only directory")
    def test_unremovable_leftovers_warn_and_still_report_the_ready_directory(self):
        script = self.script(self.serve())
        self.assertEqual(self.run_prepare(script).returncode, 0)
        downloads = self.tools / ".downloads"
        downloads.mkdir()
        (downloads / ("0" * 64 + ".part")).write_bytes(b"partial")
        stale = self.tools / ".tmp-killed"
        stale.mkdir()
        # The user cannot remove entries from a parent it cannot write.
        self.tools.chmod(0o555)
        self.addCleanup(self.tools.chmod, 0o755)
        result = self.run_prepare(script)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines()[-1], str(self.ready))
        self.assertIn(f"could not remove {downloads}", result.stderr)
        self.assertIn(f"could not remove {stale}", result.stderr)

    def test_clean_ready_directory_does_not_wait_for_the_lock(self):
        script = self.script(self.serve())
        self.assertEqual(self.run_prepare(script).returncode, 0)
        with open(self.tools / ".lock", "w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            result = subprocess.run(["bash", str(script), str(self.cache)], capture_output=True,
                                    text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.splitlines()[-1], str(self.ready))

    def test_hash_mismatch_is_refused_and_leaves_nothing(self):
        result = self.run_prepare(self.script(self.serve(), sha="0" * 64))
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("hash mismatch", result.stderr)
        self.assert_no_partial_result()
        self.assertEqual(list((self.tools / ".downloads").iterdir()), [])

    def test_release_without_proton_leaves_no_partial_directory(self):
        self.archive = release(self.root / "broken", files={"toolmanifest.vdf": '"manifest" {}\n'})
        result = self.run_prepare(self.script(self.serve()))
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertIn("no proton or toolmanifest.vdf", result.stderr)
        self.assert_no_partial_result()

    def test_interrupted_run_work_directory_is_removed(self):
        stale = self.tools / ".tmp-interrupted/files"
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
        self.assertEqual(self.part().stat().st_size, PARTIAL)
        second = self.run_prepare(script)
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(server.requests, [(ARCHIVE, None), (ARCHIVE, f"bytes={PARTIAL}-")])

    def test_partial_download_restarts_when_the_server_cannot_resume(self):
        server = self.serve(ranges=False)
        self.part().parent.mkdir(parents=True)
        self.part().write_bytes(self.archive[:PARTIAL])
        result = self.run_prepare(self.script(server))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("server cannot resume; restarting", result.stderr)
        self.assertEqual(server.requests, [(ARCHIVE, f"bytes={PARTIAL}-"), (ARCHIVE, None)])

    def test_corrupt_partial_download_restarts(self):
        server = self.serve()
        self.part().parent.mkdir(parents=True)
        self.part().write_bytes(b"\0" * PARTIAL)
        result = self.run_prepare(self.script(server))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("do not match; restarting", result.stderr)

    def test_usage_errors_exit_2(self):
        script = self.script(self.serve())
        for arguments in ([], ["relative"], [str(self.root / "missing")], [str(self.cache), "extra"]):
            with self.subTest(arguments=arguments):
                result = subprocess.run(["bash", str(script), *arguments], capture_output=True,
                                        text=True, timeout=10)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertIn("usage: cachyos-prepare <cache-root>", result.stderr)
                self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()

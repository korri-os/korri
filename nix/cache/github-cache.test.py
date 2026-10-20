#!/usr/bin/env python3
"""Run the actual uploader against a file-backed release API in child processes."""

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("github_cache", HERE / "github-cache.py")
assert SPEC is not None and SPEC.loader is not None
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)
REPO = "korri-os/nix-cache"
TAG = "batch-test"
BASE = f"https://github.com/{REPO}/releases/download/{TAG}/"


def run(*args):
    return subprocess.check_output(args, text=True, stderr=subprocess.PIPE).strip()


class ConcurrentUploadTest(unittest.TestCase):
    nar: Path
    note: Path

    @classmethod
    def setUpClass(cls):
        fixture = tempfile.TemporaryDirectory(prefix="korri-upload-signed-")
        cls.addClassCleanup(fixture.cleanup)
        root = Path(fixture.name)
        key = root / "cache.secret"
        run(
            "nix-store",
            "--generate-binary-cache-key",
            "korri-upload-test",
            str(key),
            str(root / "cache.pub"),
        )
        key.chmod(0o600)
        source = root / "korri-upload-payload"
        source.write_text("concurrent upload payload\n")
        path = run("nix", "store", "add-file", str(source))
        cache = root / "cache"
        run(
            "nix",
            "copy",
            "--max-jobs",
            "0",
            "--option",
            "builders",
            "",
            "--to",
            f"{cache.as_uri()}?compression=zstd&secret-key={key}",
            path,
        )
        prepared = root / "prepared"
        publisher.prepare(cache, prepared, BASE, [])
        cls.nar = next((prepared / "nars").iterdir())
        cls.note = next((prepared / "metadata").glob("*.narinfo"))

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="korri-upload-race-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)

    def upload(self, first, second=None, config=None):
        # Each call is an isolated real API store, not a saved response sequence.
        root = Path(tempfile.mkdtemp(dir=self.root))
        (root / "objects").mkdir()
        (root / "assets.json").write_text("{}")
        (root / "release.json").write_text(
            json.dumps(
                {
                    "id": 415496246,
                    "tag_name": TAG,
                    "draft": False,
                    "immutable": False,
                }
            )
        )
        (root / "config.json").write_text(
            json.dumps({"barrier": second is not None} | (config or {}))
        )
        binary = root / "bin"
        binary.mkdir()
        gh = binary / "gh"
        gh.write_text(
            f"#!{sys.executable}\nfrom runpy import run_path\n"
            f"run_path({str(HERE / 'github-cache-test-api.py')!r}, run_name='__main__')\n"
        )
        gh.chmod(0o755)
        processes = {}
        for worker, path in enumerate([first] if second is None else [first, second]):
            environment = os.environ | {
                "PATH": str(binary) + os.pathsep + os.environ["PATH"],
                "KORRI_CACHE_TEST_API": str(root),
                "KORRI_CACHE_TEST_WORKER": str(worker),
            }
            process = subprocess.Popen(
                [sys.executable, str(Path(__file__).resolve()), "--upload", str(path)],
                env=environment,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            self.addCleanup(self.stop, process)
            processes[str(worker)] = process
        results = {}
        for worker_id, process in processes.items():
            stdout, stderr = process.communicate(timeout=20)
            results[worker_id] = (process.returncode, stdout + stderr)
        events = [
            json.loads(line)
            for line in (root / "events.jsonl").read_text().splitlines()
        ]
        self.assertTrue(all(event["method"] in ("GET", "POST") for event in events))
        posts = [event for event in events if event["method"] == "POST"]
        self.assertEqual(len(posts), len(processes), "upload must not retry writes")
        assets = json.loads((root / "assets.json").read_text())
        return root, results, posts, assets

    @staticmethod
    def stop(process):
        if process.poll() is None:
            process.kill()
            process.communicate()

    def variant(self, path, data):
        directory = Path(tempfile.mkdtemp(dir=self.root))
        variant = directory / path.name
        variant.write_bytes(data)
        return variant

    def assert_original_failure(self, results, posts, status=422):
        failed = next(event for event in posts if event["status"] == status)
        code, output = results[failed["worker"]]
        self.assertNotEqual(code, 0, output)
        self.assertIn(f"HTTP {status}", output)
        final_error = output.rstrip().splitlines()[-1]
        self.assertIn("subprocess.CalledProcessError", final_error)
        self.assertIn("--method', 'POST'", final_error)

    def test_concurrent_identical_nar_uploads_both_succeed_without_overwrite(self):
        root, results, posts, assets = self.upload(self.nar, self.nar)
        self.assertEqual(sorted(event["status"] for event in posts), [201, 422])
        for code, output in results.values():
            self.assertEqual(code, 0, output)
        asset = assets[self.nar.name]
        self.assertEqual(
            (root / "objects" / str(asset["id"])).read_bytes(), self.nar.read_bytes()
        )
        self.assertTrue(publisher.matches(asset, self.nar))

    def test_concurrent_equivalent_metadata_keeps_the_first_url(self):
        other = self.variant(
            self.note,
            self.note.read_bytes().replace(
                BASE.encode(),
                b"https://github.com/korri-os/nix-cache/releases/download/earlier/",
            ),
        )
        root, results, posts, assets = self.upload(self.note, other)
        self.assertEqual(sorted(event["status"] for event in posts), [201, 422])
        for code, output in results.values():
            self.assertEqual(code, 0, output)
        asset = assets[self.note.name]
        data = (root / "objects" / str(asset["id"])).read_bytes()
        winner = next(event["worker"] for event in posts if event["status"] == 201)
        self.assertEqual(data, (self.note if winner == "0" else other).read_bytes())
        self.assertEqual(asset["digest"], "sha256:" + hashlib.sha256(data).hexdigest())

    def test_concurrent_different_payloads_preserve_the_post_failure(self):
        for data in (
            bytes(byte ^ 1 for byte in self.nar.read_bytes()),
            self.nar.read_bytes() + b"x",
        ):
            with self.subTest(size=len(data)):
                other = self.variant(self.nar, data)
                _, results, posts, _ = self.upload(self.nar, other)
                self.assert_original_failure(results, posts)

    def test_concurrent_different_signatures_are_not_equivalent(self):
        other = self.variant(
            self.note, self.note.read_bytes().replace(b"Sig: ", b"Sig: different-")
        )
        _, results, posts, _ = self.upload(self.note, other)
        self.assert_original_failure(results, posts)

    def test_unverified_assets_and_changed_releases_preserve_the_post_failure(self):
        cases = {
            "unfinished": {"collision_asset": {"state": "starter"}},
            "wrong-size": {"collision_asset": {"size": 0}},
            "wrong-digest": {"collision_asset": {"digest": "sha256:" + "0" * 64}},
            "missing": {"remove_on_collision": True},
            "changed-id": {"collision_release": {"id": 415496247}},
            "draft": {"collision_release": {"draft": True}},
            "immutable": {"collision_release": {"immutable": True}},
            "refresh-failure": {"refresh_error": True},
        }
        for name, config in cases.items():
            with self.subTest(name=name):
                _, results, posts, _ = self.upload(self.nar, self.nar, config)
                self.assert_original_failure(results, posts)

    def test_metadata_verification_errors_preserve_the_post_failure(self):
        for fields in ({"digest": "sha256:" + "0" * 64}, {"size": "invalid"}):
            with self.subTest(fields=fields):
                _, results, posts, _ = self.upload(
                    self.note,
                    self.note,
                    {"collision_asset": fields},
                )
                self.assert_original_failure(results, posts)

    def test_unrelated_post_failure_is_not_suppressed(self):
        _, results, posts, _ = self.upload(self.nar, config={"post_error": 503})
        self.assert_original_failure(results, posts, status=503)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--upload":
        publisher.upload_files(REPO, TAG, [Path(path) for path in sys.argv[2:]])
    else:
        unittest.main(verbosity=2)

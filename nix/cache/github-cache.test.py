#!/usr/bin/env python3
"""Run the actual uploader against a file-backed release API in child processes."""

import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from urllib.parse import parse_qs, urlsplit


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


class AssetListingTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="korri-assets-pages-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.records = [self.asset(index) for index in range(202)]
        self.requests = []
        self.scans = 0
        self.change = lambda: None
        self.response = None
        test = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                url = urlsplit(self.path)
                test.requests.append(self.path)
                if url.path == f"/repos/{REPO}/releases":
                    data = [{"id": 1, "tag_name": TAG, "draft": False}]
                elif url.path == f"/repos/{REPO}/releases/1/assets":
                    query = parse_qs(url.query)
                    page = int(query["page"][0])
                    size = int(query["per_page"][0])
                    if page == 1:
                        test.scans += 1
                    records = sorted(test.records, key=lambda asset: asset["name"])
                    data = [
                        asset | {"download_count": len(test.requests)}
                        for asset in records[(page - 1) * size : page * size]
                    ]
                    # Change the live, name-ordered collection after taking a
                    # page, not a sequence of pre-recorded HTTP responses.
                    if page == 1:
                        test.change()
                    if test.response is not None:
                        data = test.response
                else:
                    self.send_error(404)
                    return
                payload = json.dumps(data).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

            def log_message(self, format, *args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        # Keep the production gh subprocess seam, with a real HTTP transport
        # to this isolated server. No GitHub credentials or remote writes.
        gh = self.root / "gh"
        gh.write_text(
            f"#!{sys.executable}\n"
            "import json, sys\nfrom urllib.request import urlopen\n"
            f"with open({str(self.root / 'commands.jsonl')!r}, 'a') as log:\n"
            "    log.write(json.dumps(sys.argv[1:]) + '\\n')\n"
            "assert sys.argv[1:4] == ['api', '--hostname', 'github.com']\n"
            "assert len(sys.argv) == 5, 'only GET requests are allowed'\n"
            f"with urlopen('http://127.0.0.1:{server.server_port}/' + sys.argv[4]) as response:\n"
            "    sys.stdout.buffer.write(response.read())\n"
        )
        gh.chmod(0o755)

    @staticmethod
    def asset(index):
        return {
            "id": index + 1,
            "name": f"asset-{index:04d}",
            "state": "uploaded",
            "size": 1,
            "digest": "sha256:" + hashlib.sha256(b"x").hexdigest(),
        }

    def invoke(self, command="--assets", *args):
        return subprocess.run(
            [sys.executable, str(Path(__file__).resolve()), command, *map(str, args)],
            env=os.environ | {"PATH": str(self.root) + os.pathsep + os.environ["PATH"]},
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=30,
        )

    def listing(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def assert_complete(self, result):
        self.assertEqual(
            {name: asset["id"] for name, asset in result.items()},
            {asset["name"]: asset["id"] for asset in self.records},
        )

    def test_stable_pages_ignore_changing_download_counts(self):
        self.assert_complete(self.listing())
        self.assertEqual(self.scans, 2)

    def test_upload_before_a_page_boundary_repeats_an_id_but_not_an_asset(self):
        def insert():
            if self.scans == 1:
                self.records.append(self.asset(-1))

        self.change = insert
        self.assert_complete(self.listing())
        self.assertEqual(self.scans, 3)

    def test_deleted_starter_skips_an_asset_until_the_next_scan(self):
        self.records[0]["state"] = "starter"

        def remove():
            if self.scans == 1:
                self.records.pop(0)

        self.change = remove
        result = self.listing()
        self.assert_complete(result)
        self.assertNotIn("asset-0000", result)
        self.assertIn("asset-0100", result)
        self.assertEqual(self.scans, 3)

    def test_uploader_verifies_an_asset_skipped_by_the_first_scan_without_post(self):
        self.records[0]["state"] = "starter"

        def remove():
            if self.scans == 1:
                self.records.pop(0)

        self.change = remove
        path = self.root / "asset-0100"
        path.write_bytes(b"x")
        result = self.invoke("--upload", path)
        self.assertEqual(result.returncode, 0, result.stderr)
        commands = [
            json.loads(line)
            for line in (self.root / "commands.jsonl").read_text().splitlines()
        ]
        self.assertTrue(all(len(command) == 4 for command in commands), commands)

    def test_different_ids_with_one_name_fail_even_across_pages(self):
        for index in (0, 99):
            with self.subTest(index=index):
                self.records = [self.asset(i) for i in range(202)]
                self.records.append(self.records[index] | {"id": 999})
                result = self.invoke()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("duplicate remote asset name", result.stderr)

    def test_integrity_field_changes_require_another_scan(self):
        for key, value in (
            ("id", 999),
            ("state", "starter"),
            ("size", 2),
            ("digest", "sha256:" + "0" * 64),
        ):
            with self.subTest(key=key):
                self.scans = 0
                self.records = [self.asset(i) for i in range(202)]

                def change():
                    if self.scans == 1:
                        self.records[0][key] = value

                self.change = change
                result = self.listing()
                self.assertEqual(result["asset-0000"][key], value)
                self.assertEqual(self.scans, 3)

    def test_continuous_changes_fail_with_a_bounded_read_only_error(self):
        def insert():
            self.records.append(self.asset(1000 + self.scans))

        self.change = insert
        result = self.invoke()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("asset listing did not stabilize", result.stderr)
        self.assertEqual(self.scans, 20)

    def test_missing_or_invalid_ids_cannot_be_treated_as_repeated_assets(self):
        for value in (None, "1", True):
            with self.subTest(value=value):
                self.records = [self.asset(0) | {"id": value}]
                result = self.invoke()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("invalid GitHub asset id", result.stderr)

    def test_invalid_list_fails(self):
        self.response = {"message": "unexpected response"}
        result = self.invoke()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("invalid GitHub asset list", result.stderr)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--upload":
        publisher.upload_files(REPO, TAG, [Path(path) for path in sys.argv[2:]])
    elif len(sys.argv) > 1 and sys.argv[1] == "--assets":
        print(json.dumps(publisher.assets(REPO, 1)))
    else:
        unittest.main(verbosity=2)
